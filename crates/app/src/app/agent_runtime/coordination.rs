use canvas::{CanvasNodeKind, NodeId};

use super::super::MultisoulApp;
use super::commands::submit_agent_task;

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
                    speech_alias: definition.speech_alias.clone(),
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
        let (terminal_state, terminal_error) = match self.runtime.agent_task_state(node_id) {
            Some(runtime::AgentTaskState::Done) => (storage::AgentDispatchTaskState::Done, None),
            Some(runtime::AgentTaskState::Failed { ref message }) => (
                storage::AgentDispatchTaskState::Failed,
                Some(message.clone()),
            ),
            Some(runtime::AgentTaskState::Ended) => (
                storage::AgentDispatchTaskState::Interrupted,
                Some("Agent session ended".to_string()),
            ),
            _ => (storage::AgentDispatchTaskState::Interrupted, None),
        };
        let writer = self.persistence.agent_dispatch_writer().cloned();
        let next = self
            .agent_task_coordinator
            .as_mut()
            .and_then(|coordinator| coordinator.finish_current(definition_id));
        if let Some(writer) = writer.as_ref() {
            writer.finish_current(definition_id, terminal_state, terminal_error);
        }
        let mut next = next;
        while let Some(candidate) = next {
            match submit_agent_task(&mut self.runtime, candidate.node_id, &candidate.task) {
                Ok(()) => {
                    if let Some(writer) = writer.as_ref() {
                        writer.submit_next_queued(definition_id);
                    }
                    return true;
                }
                Err(error) => {
                    if let Some(writer) = writer.as_ref() {
                        writer.fail_next_queued(definition_id, error.to_string());
                    }
                    tracing::error!(
                        definition_id,
                        node_id = candidate.node_id,
                        task_id = candidate.task_id,
                        %error,
                        "failed to submit next queued Agent task"
                    );
                    if !error.retry_safe() {
                        return false;
                    }
                    next = self
                        .agent_task_coordinator
                        .as_mut()
                        .and_then(|coordinator| coordinator.finish_current(definition_id));
                }
            }
        }
        false
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
        canvas::AgentProfile::Cursor => norma::protocol::AgentProfile::Cursor,
        canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
    }
}
