use super::Runtime;
use canvas::{CanvasNodeKind, NodeId};
use norma::workspace_dispatch::{
    CardExecutionFingerprint, WorkspaceCardRef, WorkspaceCardTarget, WorkspaceTargetError,
};

impl Runtime {
    pub fn runtime_instance_id(&self) -> &str {
        &self.runtime_instance_id
    }

    /// The App first authorizes and resolves the workspace/area to this Runtime.
    /// Captures memory only; never opens a session, drains a PTY or accesses storage.
    pub fn workspace_card_target(
        &self,
        workspace_id: &str,
        area_id: &str,
        route_generation: u64,
        node_id: NodeId,
    ) -> Result<WorkspaceCardTarget, WorkspaceTargetError> {
        let node = self
            .document
            .node(node_id)
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .ok_or(WorkspaceTargetError::TargetMissing)?;
        let target = WorkspaceCardTarget {
            card: WorkspaceCardRef {
                workspace_id: workspace_id.into(),
                area_id: area_id.into(),
                node_id,
                card_instance_id: node.card_instance_id.clone(),
            },
            execution: CardExecutionFingerprint {
                runtime_instance_id: self.runtime_instance_id.clone(),
                route_generation,
                terminal_session_id: self
                    .live
                    .get(&node_id)
                    .and_then(|live| live.terminal_session_id()),
                run_id: self.mobile_agent_run_id(node_id).map(str::to_owned),
                session_id: self.agent_session_id(node_id),
            },
        };
        target.validate()?;
        Ok(target)
    }

    /// Identity check only. Availability, authorization and submit policy remain separate.
    pub fn validate_workspace_card_target(
        &self,
        workspace_id: &str,
        area_id: &str,
        route_generation: u64,
        target: &WorkspaceCardTarget,
    ) -> Result<(), WorkspaceTargetError> {
        target.validate()?;
        if target.card.workspace_id != workspace_id || target.card.area_id != area_id {
            return Err(WorkspaceTargetError::RouteChanged);
        }
        let current = self.workspace_card_target(
            workspace_id,
            area_id,
            route_generation,
            target.card.node_id,
        )?;
        if target.card.card_instance_id != current.card.card_instance_id {
            return Err(WorkspaceTargetError::CardChanged);
        }
        if target.execution.route_generation != current.execution.route_generation {
            return Err(WorkspaceTargetError::RouteChanged);
        }
        if target.execution.runtime_instance_id != current.execution.runtime_instance_id {
            return Err(WorkspaceTargetError::RuntimeChanged);
        }
        if target.execution.run_id != current.execution.run_id {
            return Err(WorkspaceTargetError::RunChanged);
        }
        if target.execution.session_id != current.execution.session_id
            || target.execution.terminal_session_id != current.execution.terminal_session_id
        {
            return Err(WorkspaceTargetError::SessionChanged);
        }
        Ok(())
    }
}
