#[cfg(any(test, feature = "e2e-test-support"))]
use super::DispatchEffectCounts;
use super::{
    DispatchEffectPresentation, DispatchEffectSpawnOutcome, DispatchReceiptVisualStatus,
    MotionStore,
};
use canvas::{CanvasRect, NodeId};

impl MotionStore {
    pub(crate) fn spawn_dispatch_receipt(
        &mut self,
        task_ref: impl Into<String>,
        target_node_id: NodeId,
        status: DispatchReceiptVisualStatus,
    ) -> DispatchEffectSpawnOutcome {
        self.dispatch_receipts
            .spawn(task_ref.into(), target_node_id, status, self.reduce_motion)
    }

    pub(crate) fn dispatch_receipt_presentations(&self) -> Vec<DispatchEffectPresentation> {
        self.dispatch_receipts.presentations()
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn dispatch_receipt_counts(&self) -> DispatchEffectCounts {
        self.dispatch_receipts.counts()
    }

    pub(crate) fn current_node_rect(&self, id: NodeId, canonical_rect: CanvasRect) -> CanvasRect {
        self.nodes
            .get(&id)
            .map(|node| node.current_rect)
            .unwrap_or(canonical_rect)
    }

    pub(crate) fn has_dispatch_receipts(&self) -> bool {
        self.dispatch_receipts.is_active()
    }

    pub(crate) fn dispatch_target_node_ids(&self) -> Vec<NodeId> {
        self.dispatch_receipts.target_node_ids()
    }

    pub(crate) fn retain_dispatch_targets(&mut self, target_nodes: &[NodeId]) {
        self.dispatch_receipts
            .retain_targets(|id| target_nodes.contains(&id));
    }

    pub(crate) fn record_dispatch_unavailable_skip(&mut self) {
        self.dispatch_receipts.record_unavailable_skip();
    }
}
