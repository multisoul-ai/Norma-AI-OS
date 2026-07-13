use canvas::{CanvasNodeKind, NodeId};
use runtime::NodeInput;

use super::super::MultisoulApp;

impl MultisoulApp {
    pub(super) fn agent_roster_for_provider(&self) -> Vec<agent::AgentRosterEntry> {
        self.agent_definitions
            .definitions()
            .iter()
            .map(|definition| {
                let active_node_id = self.runtime.document.nodes().iter().find_map(|node| {
                    let CanvasNodeKind::Agent(spec) = &node.kind else {
                        return None;
                    };
                    (spec.definition_id == Some(definition.id)).then_some(node.id)
                });
                let busy = active_node_id.is_some_and(|node_id| {
                    matches!(
                        self.runtime.agent_task_state(node_id),
                        Some(runtime::AgentTaskState::Thinking)
                            | Some(runtime::AgentTaskState::RunningTool { .. })
                            | Some(runtime::AgentTaskState::WaitingApproval { .. })
                    )
                });
                agent::AgentRosterEntry {
                    definition_id: definition.id,
                    name: definition.name.clone(),
                    speech_alias: agent::builtin_agent_names()
                        .iter()
                        .find(|builtin| builtin.name().eq_ignore_ascii_case(&definition.name))
                        .map(|builtin| builtin.speech_alias().to_string()),
                    profile: norma_agent_profile(definition.runtime_profile),
                    enabled: definition.enabled,
                    active_node_id,
                    tracking: if self.agent_hook_server.runtime_config().is_some() {
                        agent::AgentTrackingMode::Managed
                    } else {
                        agent::AgentTrackingMode::Untracked
                    },
                    busy,
                }
            })
            .collect()
    }

    pub(crate) fn advance_agent_task_queue(&mut self, node_id: NodeId) -> bool {
        let Some(definition_id) = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        }) else {
            return false;
        };
        if let Some(store) = self.persistence.agent_dispatch_task_store() {
            let runtime_state = self.runtime.agent_task_state(node_id);
            let (state, error) = match runtime_state {
                Some(runtime::AgentTaskState::Done) => {
                    (storage::AgentDispatchTaskState::Done, None)
                }
                Some(runtime::AgentTaskState::Failed { ref message }) => (
                    storage::AgentDispatchTaskState::Failed,
                    Some(message.as_str()),
                ),
                Some(runtime::AgentTaskState::Ended) => (
                    storage::AgentDispatchTaskState::Interrupted,
                    Some("Agent session ended"),
                ),
                _ => (storage::AgentDispatchTaskState::Interrupted, None),
            };
            if let Err(error) = store.finish_current(definition_id, state, error) {
                tracing::error!(definition_id, %error, "failed to persist Agent terminal state");
            }
        }
        let Some(next) = self
            .agent_task_coordinator
            .as_mut()
            .and_then(|coordinator| coordinator.finish_current(definition_id))
        else {
            return false;
        };
        if let Some(store) = self.persistence.agent_dispatch_task_store() {
            if let Err(error) = store.submit_next_queued(definition_id) {
                tracing::error!(definition_id, %error, "failed to persist queued Agent submission");
            }
        }
        match self
            .runtime
            .send_node_input(next.node_id, NodeInput::Text(format!("{}\n", next.task)))
        {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(
                    definition_id,
                    node_id = next.node_id,
                    task_id = next.task_id,
                    %error,
                    "failed to submit next queued Agent task"
                );
                false
            }
        }
    }

    pub(crate) fn agent_completion_report_for_node(&self, node_id: NodeId) -> Option<String> {
        let definition_id = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        })?;
        let name = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)?
            .name
            .as_str();
        let state = self.runtime.agent_task_state(node_id)?;
        agent_completion_report(name, &state)
    }
}

pub(super) fn agent_completion_report(
    name: &str,
    state: &runtime::AgentTaskState,
) -> Option<String> {
    match state {
        runtime::AgentTaskState::Done => Some(format!("{name} 已完成任务")),
        runtime::AgentTaskState::Failed { message } => Some(format!("{name} 任务失败：{message}")),
        runtime::AgentTaskState::Ended => Some(format!("{name} 任务已中断")),
        runtime::AgentTaskState::Ready
        | runtime::AgentTaskState::Thinking
        | runtime::AgentTaskState::RunningTool { .. }
        | runtime::AgentTaskState::WaitingApproval { .. } => None,
    }
}

fn norma_agent_profile(profile: canvas::AgentProfile) -> norma::protocol::AgentProfile {
    match profile {
        canvas::AgentProfile::Claude => norma::protocol::AgentProfile::Claude,
        canvas::AgentProfile::Codex => norma::protocol::AgentProfile::Codex,
        canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
    }
}
