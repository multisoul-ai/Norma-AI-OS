use super::*;

struct ToolCallIdentityHook {
    context: RigToolContext,
}

impl<M> AgentHook<M> for ToolCallIdentityHook
where
    M: CompletionModel,
{
    async fn on_event(&self, _context: &HookContext, event: StepEvent<'_, M>) -> Flow {
        if let StepEvent::ToolCall {
            tool_name,
            args,
            tool_call_id,
            internal_call_id,
            ..
        } = event
        {
            if self.context.is_single_voice_command()
                && !crate::voice_command::is_read_only_tool(tool_name)
            {
                self.context.propose_voice_command(tool_name, args);
                return Flow::skip("{\"status\":\"proposed\",\"executed\":false}");
            }
            self.context
                .record_tool_call(tool_call_id, internal_call_id);
        }
        Flow::cont()
    }
}

pub(super) fn build_agent<M>(
    model: M,
    preamble: &str,
    tools: NormaToolRuntime,
    rig_context: RigToolContext,
    desktop_active: bool,
    context_data: Option<&str>,
) -> Agent<M>
where
    M: CompletionModel,
{
    let personal = rig_context.context_project_is_personal();
    let workspace = rig_context.is_mobile_workspace();
    let gpt_live = rig_context.is_gpt_live();
    let normalized_delete_schema = gpt_live || rig_context.is_single_voice_command();
    let builder = AgentBuilder::new(model)
        .name("Norma")
        .description("Norma OS 的唯一 Master Agent")
        .preamble(preamble)
        .add_hook(ToolCallIdentityHook {
            context: rig_context,
        })
        .tool(crate::rig_tools::MemoryListTool::new(tools.clone()))
        .tool(crate::rig_tools::MemorySearchTool::new(tools.clone()))
        .tool(crate::rig_tools::MemoryReadTool::new(tools.clone()))
        .tool(crate::rig_tools::MemoryWriteTool::new(tools.clone()));
    let builder = if let Some(data) = context_data {
        builder
            .context(data)
            .tool(crate::rig_tools::ReadSharedContextTool::new(tools.clone()))
            .tool(crate::rig_tools::UpdateWorkingStateTool::new(tools.clone()))
    } else {
        builder
    };
    let builder = if workspace {
        builder.tool(crate::rig_tools::WorkspaceMemoryTool::new(tools.clone()))
    } else {
        builder
    };
    let builder = if desktop_active {
        builder
            .tool(crate::rig_tools::WorkspaceSnapshotTool::new(tools.clone()))
            .tool(crate::rig_tools::OpenWorkspaceArtifactTool::new(
                tools.clone(),
            ))
            .tool(crate::rig_tools::LocateWorkspaceAgentTool::new(
                tools.clone(),
            ))
            .tool(crate::rig_tools::CaptureIdeaTool::new(tools.clone()))
            .tool(crate::rig_tools::ListIdeasTool::new(tools.clone()))
            .tool(AssignAgentTaskTool::new(tools.clone()))
    } else {
        builder
    };
    if personal {
        return builder.build();
    }
    let delete = DeleteCanvasCardTool::new(tools.clone());
    // 个人会话没有项目，也就没有后台大脑可交接。
    let builder = builder
        .tool(crate::rig_tools::HandoffToBrainTool::new(tools.clone()))
        .tool(FindAgentTasksTool::new(tools.clone()))
        .tool(CreateAgentTool::new(tools.clone()))
        .tool(CancelAgentTaskTool::new(tools.clone()))
        .tool(ReplyAgentTool::new(tools.clone()))
        .tool(CancelAgentReplyTool::new(tools.clone()))
        .tool(CreateTerminalTool::new(tools.clone()));
    let builder = if desktop_active {
        builder
    } else {
        builder.tool(AssignAgentTaskTool::new(tools.clone()))
    };
    if normalized_delete_schema {
        builder
            .tool(crate::rig_tools::GptLiveDeleteCanvasCardTool(delete))
            .build()
    } else {
        builder.tool(delete).build()
    }
}

impl NormaSession {
    pub(super) async fn memory_preamble(
        &mut self,
        input: &str,
        context: &ToolContext,
        frame: Option<crate::context::ContextFrame>,
    ) -> Result<(String, Option<String>), NormaSessionError> {
        let memory = self
            .tools
            .memory_context()
            .await
            .map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
        let enabled = memory
            .as_ref()
            .is_some_and(|memory| memory.preferences.recall);
        let epoch = memory.as_ref().map_or(0, |memory| memory.recall_epoch);
        let revisions = memory
            .as_ref()
            .map(|memory| memory.revisions.clone())
            .unwrap_or_default();
        if epoch != self.memory_epoch
            || (self.memory_enabled && (!enabled || revisions != self.memory_revisions))
        {
            // Historical replies/tool results may paraphrase deleted or corrected facts.
            // Without per-message provenance the whole model history is invalidated.
            self.protocol_history.clear();
        }
        self.memory_enabled = enabled;
        self.memory_epoch = epoch;
        self.memory_revisions = revisions;
        let mut canvas = self.canvas.clone();
        let conversation_tails = std::mem::take(&mut canvas.conversation_tails);
        let mut preamble = crate::norma_system_prompt_base_with_desktop(
            &canvas,
            &self.roster,
            self.desktop_context.as_ref(),
        );
        let persona = memory
            .as_ref()
            .map(|context| context.persona.clone())
            .unwrap_or_default();
        preamble.push_str(&persona.prompt());
        preamble.push_str(crate::NORMA_MEMORY_RULES);
        // Reserve reply tokens; a future tool receipt is optional headroom, not
        // part of the current request. Required state must fit before headroom.
        // Conservative serialized bytes/token=2 matches the existing history estimator.
        let total = super::request_budget::input_bytes(self.context_window_tokens);
        let shared = frame.is_some();
        let definitions = crate::norma_tool_definitions()
            .iter()
            .filter(|tool| {
                active_definition(&tool.name, context, self.desktop_context.as_ref(), shared)
            })
            .map(|tool| {
                serde_json::json!({"type":"function","function":{
                    "name":tool.name,"description":tool.description,"parameters":tool.parameters
                }})
                .to_string()
                .len()
                    + 1
            })
            .sum::<usize>();
        let input_bytes = serde_json::to_string(input)
            .expect("input serialization")
            .len();
        let reserved = definitions + input_bytes + 768;
        self.history_char_budget = total.saturating_sub(
            serde_json::to_string(&preamble)
                .expect("preamble serialization")
                .len()
                + reserved,
        );
        // Required state is allocated before the optional memory directory.
        let context_data = self.shared_context_data(context, frame)?;
        if shared {
            preamble.push_str(crate::context::SHARED_CONTEXT_RULES);
        }
        self.append_conversation_context(&mut preamble, conversation_tails);
        if let Some(memory) = memory.filter(|memory| memory.preferences.recall) {
            // Optional catalog yields tool-result headroom; conversation history
            // can use the actual remaining window and is pruned by whole turns.
            let available = self
                .history_char_budget
                .saturating_sub(crate::MEMORY_TOOL_BYTES)
                .min(crate::MEMORY_CATALOG_BYTES);
            let mut raw_budget = available;
            while raw_budget > 0 {
                let Ok((catalog, next)) = crate::memory_text_chunk(&memory.catalog, 0, raw_budget)
                else {
                    break;
                };
                let suffix = if next.is_some() {
                    "\n目录不完整，请使用 memory_list/search。\n"
                } else {
                    ""
                };
                let addition = format!("{catalog}{suffix}");
                let bytes = serde_json::to_string(&addition)
                    .expect("catalog serialization")
                    .len();
                if bytes <= available {
                    preamble.push_str(&addition);
                    self.history_char_budget = self.history_char_budget.saturating_sub(bytes);
                    break;
                }
                raw_budget /= 2;
            }
        }
        self.prune_protocol_history();
        crate::trace_assembled_prompt("session", &preamble);
        if let Some(data) = &context_data {
            crate::trace_assembled_prompt("session-shared", data);
        }
        Ok((preamble, context_data))
    }
}

fn active_definition(
    name: &str,
    context: &ToolContext,
    desktop: Option<&DesktopContext>,
    shared: bool,
) -> bool {
    let rig = RigToolContext::new(context.clone());
    crate::model_tool_visible(
        name,
        crate::ModelToolScope {
            personal: rig.context_project_is_personal(),
            desktop: matches!(
                desktop,
                Some(
                    DesktopContext::WorkspaceOverview { .. }
                        | DesktopContext::ProjectCanvas { .. }
                        | DesktopContext::Switching { .. }
                )
            ),
            shared,
            mobile: rig.is_mobile_workspace(),
        },
    )
}
