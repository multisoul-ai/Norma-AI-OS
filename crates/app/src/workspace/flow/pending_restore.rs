use super::session_restore_plan;
use canvas::NodeId;
use runtime::Runtime;
use std::path::PathBuf;
use storage::{NodeSession, SessionCheckpoint};

#[derive(Clone, Debug)]
pub(crate) struct PendingLiveSessionRestore {
    pub(super) root: PathBuf,
    pub(super) checkpoints: Vec<SessionCheckpoint>,
    pub(super) node_sessions: Vec<NodeSession>,
    pub(super) node_ids: Vec<NodeId>,
    pub(super) legacy_discovery_node_ids: Vec<NodeId>,
}

impl PendingLiveSessionRestore {
    pub(crate) fn new(
        root: PathBuf,
        runtime: &Runtime,
        checkpoints: Vec<SessionCheckpoint>,
        node_sessions: Vec<NodeSession>,
    ) -> Self {
        let node_ids = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    canvas::CanvasNodeKind::Term(_) | canvas::CanvasNodeKind::Agent(_)
                )
            })
            .map(|node| node.id)
            .collect();
        let legacy_discovery_node_ids = session_restore_plan::legacy_discovery_node_ids(runtime);
        Self {
            root,
            checkpoints,
            node_sessions,
            node_ids,
            legacy_discovery_node_ids,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.node_ids.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.node_ids.is_empty()
    }
}
