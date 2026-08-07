use canvas::{CanvasNodeKind, NodeId};

use super::super::super::MultisoulApp;

impl MultisoulApp {
    pub(crate) fn advance_agent_task_queue(&mut self, node_id: NodeId) -> bool {
        self.advance_agent_task_queue_with_result(node_id, None)
    }

    pub(crate) fn advance_agent_task_queue_with_result(
        &mut self,
        node_id: NodeId,
        result: Option<norma::protocol::AgentTaskResult>,
    ) -> bool {
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
            let coordinator_changed = self
                .agent_task_coordinator
                .as_mut()
                .and_then(|coordinator| {
                    coordinator.finish_all_with_state(definition_id, "interrupted")
                })
                .is_some_and(|(was_busy, cancelled_queued)| was_busy || cancelled_queued > 0);
            self.pending_agent_submissions.remove_node(node_id);
            if let Some(writer) = self.persistence.agent_dispatch_writer() {
                writer.interrupt_definition(definition_id);
            }
            return coordinator_changed;
        }
        let cancel_requested = self
            .agent_task_coordinator
            .as_ref()
            .is_some_and(|coordinator| coordinator.is_cancel_requested(definition_id));
        let (terminal_state, coordinator_state, terminal_error) = match runtime_state {
            Some(runtime::AgentTaskState::Done) if cancel_requested => (
                storage::AgentDispatchTaskState::Interrupted,
                "interrupted",
                None,
            ),
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
        let result = if terminal_state == storage::AgentDispatchTaskState::Done {
            result
        } else {
            Some(norma::protocol::AgentTaskResult {
                availability: norma::protocol::AgentTaskResultAvailability::Unavailable,
                text: None,
                truncated: false,
            })
        };
        let stored_result = result
            .as_ref()
            .map(|value| storage::StoredAgentDispatchResult {
                text: value.text.clone(),
                truncated: value.truncated,
            });
        let writer = self.persistence.agent_dispatch_writer().cloned();
        let next = self
            .agent_task_coordinator
            .as_mut()
            .and_then(|coordinator| {
                coordinator.finish_current_with_result(definition_id, coordinator_state, result)
            });
        if let Some(writer) = writer.as_ref() {
            writer.finish_current_with_result(
                definition_id,
                terminal_state,
                terminal_error,
                stored_result,
            );
        }
        if let Some(candidate) = next {
            self.pending_agent_submissions.enqueue_immediate(candidate);
            return true;
        }
        false
    }
}
