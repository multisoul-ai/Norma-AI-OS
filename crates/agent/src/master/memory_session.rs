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
            tool_call_id,
            internal_call_id,
            ..
        } = event
        {
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
) -> Agent<M>
where
    M: CompletionModel,
{
    let personal = rig_context.context_project_is_personal();
    let workspace = rig_context.is_mobile_workspace();
    let gpt_live = rig_context.is_gpt_live();
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
    let builder = if workspace {
        builder.tool(crate::rig_tools::WorkspaceMemoryTool::new(tools.clone()))
    } else {
        builder
    };
    let builder = if desktop_active {
        builder
            .tool(crate::rig_tools::WorkspaceSnapshotTool::new(tools.clone()))
            .tool(crate::rig_tools::AssignWorkspaceTaskTool::new(
                tools.clone(),
            ))
            .tool(crate::rig_tools::OpenWorkspaceArtifactTool::new(
                tools.clone(),
            ))
            .tool(crate::rig_tools::LocateWorkspaceAgentTool::new(
                tools.clone(),
            ))
    } else {
        builder
    };
    let builder = if desktop_active && personal {
        builder
            .tool(CreateBrowserTool::new(tools.clone()))
            .tool(CreatePreviewTool::new(tools.clone()))
    } else {
        builder
    };
    let builder = if desktop_active && personal {
        builder
            .tool(CreateBrowserTool::new(tools.clone()))
            .tool(CreatePreviewTool::new(tools.clone()))
    } else {
        builder
    };
    if personal {
        return builder.build();
    }
    let delete = DeleteCanvasCardTool::new(tools.clone());
    let builder = builder
        .tool(FindAgentTasksTool::new(tools.clone()))
        .tool(CreateAgentTool::new(tools.clone()))
        .tool(AssignAgentTaskTool::new(tools.clone()))
        .tool(CancelAgentTaskTool::new(tools.clone()))
        .tool(ReplyAgentTool::new(tools.clone()))
        .tool(CancelAgentReplyTool::new(tools.clone()))
        .tool(CreateTerminalTool::new(tools.clone()))
        .tool(CreateBrowserTool::new(tools.clone()))
        .tool(CreatePreviewTool::new(tools.clone()))
        .tool(FocusCardTool::new(tools.clone()))
        .tool(SendTerminalInputTool::new(tools));
    if gpt_live {
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
    ) -> Result<String, NormaSessionError> {
        let memory = self
            .tools
            .memory_context()
            .await
            .map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
        let enabled = memory
            .as_ref()
            .is_some_and(|memory| memory.preferences.recall);
        let epoch = memory.as_ref().map_or(0, |memory| memory.recall_epoch);
        if epoch != self.memory_epoch || (self.memory_enabled && !enabled) {
            self.protocol_history.clear();
        }
        self.memory_enabled = enabled;
        self.memory_epoch = epoch;
        let mut preamble = crate::norma_system_prompt_base_with_desktop(
            &self.canvas,
            &self.roster,
            self.desktop_context.as_ref(),
        );
        let persona = memory
            .as_ref()
            .map(|context| context.persona.clone())
            .unwrap_or_default();
        preamble.push_str(&persona.prompt());
        preamble.push_str(crate::NORMA_MEMORY_RULES);
        // Reserve reply(2048 tokens) + one bounded tool envelope(8192 bytes).
        // Conservative serialized bytes/token=2 matches the existing history estimator.
        let total = self
            .context_window_tokens
            .saturating_sub(2048)
            .saturating_mul(2);
        let definitions = crate::norma_tool_definitions()
            .iter()
            .map(|tool| {
                tool.description.len() + tool.parameters.to_string().len() + tool.name.len() + 80
            })
            .sum::<usize>();
        let memory_budget = total
            .saturating_sub(
                preamble.len() + definitions + input.len() + crate::MEMORY_TOOL_BYTES + 140,
            )
            .min(crate::MEMORY_CATALOG_BYTES);
        if let Some(memory) = memory.filter(|memory| memory.preferences.recall) {
            if memory_budget > 0 {
                let (catalog, next) =
                    crate::memory_text_chunk(&memory.catalog, 0, memory_budget)
                        .map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
                preamble.push_str(&catalog);
                if next.is_some() {
                    preamble.push_str("\n目录不完整，请使用 memory_list/search。\n");
                }
            }
        }
        self.history_char_budget = total
            .saturating_sub(preamble.len() + definitions + input.len() + crate::MEMORY_TOOL_BYTES);
        self.prune_protocol_history();
        Ok(preamble)
    }
}
