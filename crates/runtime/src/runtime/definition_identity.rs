use super::Runtime;
use canvas::{AgentProfile, CanvasNodeKind, CanvasRect, NodeId};
use terminal::PtyError;

impl Runtime {
    pub fn spawn_agent_for_definition(
        &mut self,
        profile: AgentProfile,
        definition_id: i64,
        alias: String,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        if let Some(existing) = self.document.nodes().iter().find(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.definition_id == Some(definition_id)
            )
        }) {
            return Ok(existing.id);
        }
        self.spawn_agent_with_identity(
            profile,
            Some(definition_id),
            Some(alias),
            bounds,
            cols,
            rows,
        )
    }

    pub fn has_active_agent_definition(&self, definition_id: i64) -> bool {
        self.document.nodes().iter().any(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.definition_id == Some(definition_id)
            )
        })
    }

    pub fn sync_agent_definition_name(&mut self, definition_id: i64, name: &str) -> bool {
        let Some(node) = self.document.nodes().iter().find(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.definition_id == Some(definition_id)
            )
        }) else {
            return false;
        };
        let node_id = node.id;
        let Some(node) = self.document.node_mut(node_id) else {
            return false;
        };
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            return false;
        };
        spec.alias = Some(name.to_string());
        true
    }
}
