use super::*;
use crate::NodeRunState;
use std::time::Instant;

impl Runtime {
    /// Reserve a fresh card; persistence must succeed before the caller starts its terminal.
    pub fn create_staff_worker(
        &mut self,
        profile: AgentProfile,
        alias: String,
        bounds: CanvasRect,
        dispatch_id: String,
    ) -> NodeId {
        let node = self.add_agent_node(profile, None, Some(alias), bounds);
        self.staff_dispatches.insert(node, dispatch_id);
        node
    }

    pub fn staff_dispatch_id(&self, node: NodeId) -> Option<&str> {
        self.staff_dispatches.get(&node).map(String::as_str)
    }

    pub fn accepts_staff_hook(&self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        self.staff_dispatch_id(event.node_id)
            .is_none_or(|dispatch| {
                match event.raw_json.get("_norma_dispatch_id") {
                    Some(id) => id.as_str() == Some(dispatch),
                    // Installed pre-Staff helpers already forward the per-process run ID.
                    // Accept that exact owned process; never infer identity from node/session alone.
                    None => event.control_run_id.as_deref().is_some_and(|run| {
                        self.native_question_process_id(event.node_id) == Some(run)
                    }),
                }
            })
    }

    pub fn last_output_at(&self, node: NodeId) -> Option<Instant> {
        self.live.get(&node).and_then(|live| live.last_output_at())
    }

    pub fn terminal_has_exited(&self, node: NodeId) -> bool {
        matches!(
            self.node_run_state(node),
            Some(
                NodeRunState::Finished { .. } | NodeRunState::Error { .. } | NodeRunState::Stopped
            )
        )
    }
}
