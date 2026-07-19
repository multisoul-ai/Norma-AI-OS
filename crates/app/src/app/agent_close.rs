use canvas::{CanvasNodeKind, NodeId};
use runtime::AgentTaskState;

use super::MultisoulApp;

impl MultisoulApp {
    pub(super) fn confirm_agent_close_if_needed(&mut self, node_id: NodeId) -> bool {
        let definition_id = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        });
        let Some(definition_id) = definition_id else {
            self.pending_agent_close_confirmation = None;
            return true;
        };
        let runtime_busy = matches!(
            self.runtime.agent_task_state(node_id),
            Some(
                AgentTaskState::Thinking
                    | AgentTaskState::RunningTool { .. }
                    | AgentTaskState::WaitingApproval { .. }
            )
        );
        let coordinated_busy = self
            .agent_task_coordinator
            .as_ref()
            .is_some_and(|coordinator| {
                coordinator.is_busy(definition_id)
                    || coordinator.queued_task_count(definition_id) > 0
            });
        if runtime_busy || coordinated_busy {
            if self.pending_agent_close_confirmation != Some(node_id) {
                self.pending_agent_close_confirmation = Some(node_id);
                self.workspace_state.error = Some(
                    "Agent is working or has queued tasks. Close it again to interrupt all tasks."
                        .to_string(),
                );
                return false;
            }
            if let Some(coordinator) = self.agent_task_coordinator.as_mut() {
                let _ = coordinator.cancel_all(definition_id);
            }
            if let Some(writer) = self.persistence.agent_dispatch_writer() {
                writer.interrupt_definition(definition_id);
            }
        }
        self.pending_agent_submissions.remove_node(node_id);
        self.pending_agent_close_confirmation = None;
        true
    }
}
