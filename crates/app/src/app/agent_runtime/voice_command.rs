mod bound_target;
use super::*;
use agent::voice_command::{validate_voice_commit_scope, VoiceFeedback};
use agent::{AgentInputSource, AgentRosterEntry, MemoryCommand, ToolContext};
pub(super) use bound_target::with_target;
use canvas::CanvasNodeKind;

impl MultisoulApp {
    pub(crate) fn invalidate_single_voice_commands(&mut self) {
        if let Some(runtime) = &self.agent_runtime {
            runtime.cancel_handle.cancel_voice();
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Legacy
            && self.voice_settings.values().interaction_mode()
                == crate::settings::voice::VoiceInteractionMode::SingleVoiceCommand
        {
            self.dialogue.stop_shell();
        }
    }

    /// Unlike the model's bounded canvas summary, this directory preserves every instance.
    pub(crate) fn single_voice_roster(&self) -> Vec<AgentRosterEntry> {
        let mut targets = Vec::new();
        for member in self.agent_roster_for_provider() {
            let nodes: Vec<_> = self.runtime.document.nodes().iter().filter(|node| {
                matches!(&node.kind, CanvasNodeKind::Agent(spec) if spec.definition_id == Some(member.definition_id))
            }).collect();
            if nodes.is_empty() {
                targets.push(member);
                continue;
            }
            for node in nodes {
                let mut instance = member.clone();
                instance.active_node_id = Some(node.id);
                let state = self.runtime.agent_task_state(node.id);
                instance.task_state =
                    super::coordination::agent_task_state_for_norma(state.as_ref());
                instance.busy = instance.task_state == agent::AgentTaskState::Busy;
                targets.push(instance);
            }
        }
        targets
    }
}

fn read_only(command: &AdapterCommand) -> bool {
    matches!(
        command,
        AdapterCommand::ContextCheckpoint { .. }
            | AdapterCommand::ReadSharedContext { .. }
            | AdapterCommand::FindAgentTasks { .. }
            | AdapterCommand::WorkspaceSnapshot { .. }
            | AdapterCommand::ListIdeas
            | AdapterCommand::Memory(
                MemoryCommand::Activate
                    | MemoryCommand::List { .. }
                    | MemoryCommand::Search { .. }
                    | MemoryCommand::Read { .. }
            )
    )
}

pub(super) fn validate(
    app: &mut MultisoulApp,
    context: &ToolContext,
    command: &AdapterCommand,
) -> Result<(), AdapterError> {
    let AgentInputSource::SingleVoiceCommand { generation } = context.input_source else {
        return Ok(());
    };
    if app.voice_settings.values().desktop_mode()
        != crate::settings::voice::DesktopVoiceMode::Legacy
        || app.voice_settings.values().interaction_mode()
            != crate::settings::voice::VoiceInteractionMode::SingleVoiceCommand
    {
        return Err(AdapterError::VoiceCommand(VoiceFeedback::ScopeChanged));
    }

    let project = app
        .workspace_state
        .current_root
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    if project != context.project_id
        || generation != app.dialogue.reply_generation
        || context.cancellation.is_cancelled()
    {
        return Err(AdapterError::VoiceCommand(VoiceFeedback::ScopeChanged));
    }
    let expected_scope = app
        .agent_runtime
        .as_ref()
        .and_then(|runtime| runtime.context_scope.clone());
    if expected_scope.as_ref() != Some(app.shared_context_service()?.scope()) {
        return Err(AdapterError::VoiceCommand(VoiceFeedback::ScopeChanged));
    }
    if let Some(commit) = context.voice_commit.as_deref() {
        if matches!(
            command,
            AdapterCommand::DeleteAgentCard { .. }
                | AdapterCommand::CancelAgentReply { reply_id: None, .. }
        ) && commit.binding.as_ref().is_some_and(|binding| {
            app.single_voice_roster()
                .iter()
                .filter(|target| {
                    target.enabled
                        && target.name.eq_ignore_ascii_case(&binding.name)
                        && target.active_node_id.is_some()
                })
                .count()
                > 1
        }) {
            return Err(AdapterError::VoiceCommand(VoiceFeedback::TargetMismatch));
        }

        validate_voice_commit_scope(
            context,
            &project,
            app.dialogue.reply_generation,
            commit,
            &app.single_voice_roster(),
        )
        .map_err(AdapterError::VoiceCommand)?;
        let target_matches = match command {
            AdapterCommand::CancelAgentTask { task_id } => app
                .agent_task_coordinator
                .as_ref()
                .and_then(|c| c.tasks().into_iter().find(|t| t.task_id == *task_id))
                .zip(commit.binding.as_ref())
                .is_some_and(|(task, binding)| {
                    task.definition_id == binding.definition_id
                        && Some(task.target_node_id) == binding.node_id
                }),
            AdapterCommand::CancelAgentReply {
                reply_id: Some(id),
                agent,
            } => app
                .session_reply_queue
                .peek_reply(*id)
                .zip(commit.binding.as_ref())
                .is_some_and(|(reply, binding)| {
                    reply.definition_id == binding.definition_id
                        && Some(reply.node_id) == binding.node_id
                        && agent
                            .as_deref()
                            .is_none_or(|name| name.eq_ignore_ascii_case(&binding.name))
                }),
            AdapterCommand::CancelAgentReply {
                reply_id: None,
                agent,
            } => agent
                .as_deref()
                .zip(commit.binding.as_ref())
                .is_some_and(|(name, binding)| name.eq_ignore_ascii_case(&binding.name)),
            _ => true,
        };
        if !target_matches {
            return Err(AdapterError::VoiceCommand(VoiceFeedback::TargetMismatch));
        }
    } else if !read_only(command) {
        return Err(AdapterError::VoiceCommand(VoiceFeedback::InvalidProposal));
    }
    Ok(())
}

pub(super) fn feedback_text(feedback: &VoiceFeedback) -> String {
    match feedback {
        VoiceFeedback::NewTaskForbidden => ui::i18n::text("voice.command_no_new_task").into(),
        VoiceFeedback::TargetMismatch => ui::i18n::text("voice.command_target_mismatch").into(),
        VoiceFeedback::NoActionReceipt => ui::i18n::text("voice.command_no_action_receipt").into(),
        VoiceFeedback::InvalidProposal => ui::i18n::text("voice.command_invalid_proposal").into(),
        VoiceFeedback::MissingSession => ui::i18n::text("voice.command_missing_session").into(),
        VoiceFeedback::Submission {
            completed,
            total,
            uncertain,
        } => ui::i18n::format(
            if *uncertain {
                "voice.command_submission_unknown"
            } else if completed == total {
                "voice.command_submission_received"
            } else {
                "voice.command_submission_stopped"
            },
            &[
                ("completed", completed.to_string()),
                ("total", total.to_string()),
            ],
        ),
        VoiceFeedback::ScopeChanged => ui::i18n::text("voice.command_scope_changed").into(),
        VoiceFeedback::ClarifyTarget { candidates } => {
            if candidates.is_empty() {
                ui::i18n::text("voice.command_clarify_empty_canvas").into()
            } else {
                ui::i18n::format(
                    "voice.command_clarify_target",
                    &[("agents", candidates.join(" / "))],
                )
            }
        }
    }
}

pub(super) fn localize_receipt(
    source: AgentInputSource,
    command: &AdapterCommand,
    mut result: AdapterResult,
) -> AdapterResult {
    if !matches!(source, AgentInputSource::SingleVoiceCommand { .. }) {
        return result;
    }
    let key = match (result.status.as_str(), result.reply_status.as_deref()) {
        ("accepted" | "succeeded", Some("sent")) => "voice.command_reply_sent",
        ("accepted" | "succeeded", Some("queued")) => "voice.command_reply_queued",
        ("accepted" | "succeeded", None | Some("cancelled")) => "voice.command_operation_received",
        _ => "voice.command_operation_unconfirmed",
    };
    let name = match command {
        AdapterCommand::AssignAgentTask { agent, .. }
        | AdapterCommand::ReplyAgent { agent, .. } => agent.as_str(),
        _ => "",
    };
    result.message = ui::i18n::format(key, &[("agent", name.to_owned())]);
    result
}
