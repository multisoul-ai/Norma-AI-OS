use canvas::{CanvasNodeKind, NodeId};
use runtime::InputMode;

use super::super::MultisoulApp;

impl MultisoulApp {
    pub(crate) fn current_canvas_context_for_provider(
        &self,
    ) -> Option<agent::CurrentCanvasContext> {
        let workspace_root = self.runtime.workspace_root()?;
        let project_name = workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| workspace_root.display().to_string());
        let nodes = self.runtime.document.nodes();
        let total_cards = nodes.len();
        let mut candidates = Vec::with_capacity(agent::MAX_CURRENT_CANVAS_CARDS);
        for node in nodes {
            let kind = canvas_card_kind(&node.kind);
            let selected = self.runtime.selection.contains(node.id);
            let focused = matches!(
                self.runtime.focus.mode(),
                InputMode::NodeFocused(node_id) if node_id == node.id
            );
            let key = (canvas_card_priority(kind, selected, focused), node.id);
            let candidate = (key, node, kind, selected, focused);
            if candidates.len() < agent::MAX_CURRENT_CANVAS_CARDS {
                candidates.push(candidate);
                continue;
            }
            let Some((worst_index, worst)) = candidates
                .iter()
                .enumerate()
                .max_by_key(|(_, current)| current.0)
            else {
                continue;
            };
            if key < worst.0 {
                candidates[worst_index] = candidate;
            }
        }
        candidates.sort_by_key(|candidate| candidate.0);
        let cards = candidates
            .into_iter()
            .map(
                |(_, node, kind, selected, focused)| agent::CanvasCardContext {
                    node_id: node.id,
                    kind,
                    title: node.kind.display_title(),
                    selected,
                    focused,
                },
            )
            .collect();
        Some(agent::CurrentCanvasContext {
            project_name,
            workspace_root: workspace_root.display().to_string(),
            total_cards,
            cards,
            conversation_tails: self.conversation_tails_for_context(),
        })
    }

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
        let runtime_state = self.runtime.agent_task_state(node_id);
        if matches!(runtime_state, Some(runtime::AgentTaskState::Ended)) {
            let cancellation_confirmed =
                self.agent_task_coordinator
                    .as_ref()
                    .is_some_and(|coordinator| {
                        coordinator.tasks().into_iter().any(|task| {
                            task.definition_id == definition_id && task.state == "cancel_requested"
                        })
                    });
            let coordinator_changed = self
                .agent_task_coordinator
                .as_mut()
                .and_then(|coordinator| coordinator.cancel_all(definition_id))
                .is_some_and(|(was_busy, cancelled_queued)| was_busy || cancelled_queued > 0);
            self.pending_agent_submissions.remove_node(node_id);
            if let Some(writer) = self.persistence.agent_dispatch_writer() {
                if cancellation_confirmed {
                    writer.finish_current(
                        definition_id,
                        storage::AgentDispatchTaskState::Cancelled,
                        None,
                    );
                }
                writer.interrupt_definition(definition_id);
            }
            return coordinator_changed;
        }
        let (terminal_state, coordinator_state, terminal_error) = match runtime_state {
            Some(runtime::AgentTaskState::Done) => {
                (storage::AgentDispatchTaskState::Done, "succeeded", None)
            }
            Some(runtime::AgentTaskState::Failed { message }) => (
                storage::AgentDispatchTaskState::Failed,
                "failed",
                Some(message),
            ),
            _ => (
                storage::AgentDispatchTaskState::Interrupted,
                "interrupted",
                None,
            ),
        };
        let writer = self.persistence.agent_dispatch_writer().cloned();
        let next = self
            .agent_task_coordinator
            .as_mut()
            .and_then(|coordinator| {
                coordinator.finish_current_with_state(definition_id, coordinator_state)
            });
        if let Some(writer) = writer.as_ref() {
            writer.finish_current(definition_id, terminal_state, terminal_error);
        }
        if let Some(candidate) = next {
            self.pending_agent_submissions.enqueue_immediate(candidate);
            return true;
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

fn canvas_card_kind(kind: &CanvasNodeKind) -> agent::CanvasCardKind {
    match kind {
        CanvasNodeKind::Rect(_) => agent::CanvasCardKind::Rect,
        CanvasNodeKind::Term(_) => agent::CanvasCardKind::Terminal,
        CanvasNodeKind::Agent(_) => agent::CanvasCardKind::Agent,
        CanvasNodeKind::Preview(_) => agent::CanvasCardKind::Preview,
        CanvasNodeKind::Browser(_) => agent::CanvasCardKind::Browser,
    }
}

fn canvas_card_priority(kind: agent::CanvasCardKind, selected: bool, focused: bool) -> u8 {
    if focused {
        0
    } else if kind == agent::CanvasCardKind::Agent {
        1
    } else if selected {
        2
    } else {
        3
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
        canvas::AgentProfile::Grok => norma::protocol::AgentProfile::Grok,
        canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
    }
}
