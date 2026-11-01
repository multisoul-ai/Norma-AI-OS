//! The native Provider turn owns completion, including silent API and background-terminal waits.
use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, AgentTaskUpdateOutcome};
use norma::agent_hook::AgentHookEvent;
use norma::codex::relay::CodexTurnEvent;
use serde_json::json;

impl Runtime {
    pub(in crate::runtime) fn native_codex_turn_blocks_hook(&self, event: &AgentHookEvent) -> bool {
        event.profile == norma::protocol::AgentProfile::Codex
            && matches!(
                event.provider_event.as_str(),
                "Stop" | "StopFailure" | "turn/completed"
            )
            && self
                .questions
                .native_hosts
                .get(&event.node_id)
                .is_some_and(|owned| {
                    owned.finished_turn.is_none()
                        || event.turn_id.as_ref() != owned.finished_turn.as_ref()
                })
    }

    pub(super) fn observe_native_codex_turn(
        &mut self,
        node: NodeId,
        turn: &CodexTurnEvent,
    ) -> bool {
        if !self.questions.connections.get(&node).is_some_and(|target| {
            target.connection_id == turn.attachment.connection_id()
                && target.session_id == turn.attachment.thread_id()
        }) {
            return false;
        }
        let Some(owned) = self.questions.native_hosts.get_mut(&node) else {
            return false;
        };
        let process = owned.host.process_id().to_owned();
        let to = match turn.status.as_deref() {
            None => {
                owned.active_turn = Some(turn.turn_id.clone());
                owned.finished_turn = None;
                // The real submit Hook owns prompt text and the brain's goal context.
                return true;
            }
            Some(status) if owned.active_turn.as_deref() == Some(&turn.turn_id) => {
                owned.active_turn = None;
                owned.finished_turn = Some(turn.turn_id.clone());
                match status {
                    "completed" => AgentTaskState::Done,
                    "interrupted" => AgentTaskState::Ended,
                    "failed" => AgentTaskState::Failed {
                        message: turn
                            .message
                            .clone()
                            .unwrap_or_else(|| "Codex turn failed".into()),
                    },
                    _ => return false,
                }
            }
            _ => return false,
        };
        let name = "turn/completed";
        let mut raw = json!({"turn":{"id":turn.turn_id,"status":turn.status}});
        if let Some(message) = &turn.message {
            raw["last_assistant_message"] = json!(message);
        }
        let event = AgentHookEvent {
            control_run_id: Some(process),
            node_id: node,
            profile: norma::protocol::AgentProfile::Codex,
            provider_event: name.into(),
            session_id: Some(turn.attachment.thread_id().into()),
            turn_id: Some(turn.turn_id.clone()),
            tool_name: None,
            message: turn.message.clone(),
            raw_json: raw,
        };
        if !self.observe_mobile_hook(&event) {
            return false;
        }
        let outcome = self.apply_agent_task_update_with_outcome(AgentTaskUpdate {
            node_id: node,
            profile: canvas::AgentProfile::Codex,
            provider_event: name.into(),
            to,
            task_title: None,
            session_id: event.session_id.clone(),
        });
        if outcome.changed() {
            self.questions.native_task_updates.push((event, outcome));
        }
        outcome.changed()
    }

    /// Already applied native lifecycle updates; the App publishes their usual completion effects.
    pub fn take_native_agent_task_updates(
        &mut self,
    ) -> Vec<(AgentHookEvent, AgentTaskUpdateOutcome)> {
        std::mem::take(&mut self.questions.native_task_updates)
    }
}
