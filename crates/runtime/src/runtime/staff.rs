use super::*;

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
                event
                    .raw_json
                    .get("_norma_dispatch_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(dispatch)
            })
    }
}
