#[cfg(any(test, feature = "e2e-test-support"))]
use super::DispatchEffectCounts;
use super::{
    DispatchEffectPresentation, DispatchEffectSpawnOutcome, DispatchReceiptVisualStatus,
    DispatchVisualOrigin, MotionStore,
};
use canvas::{CanvasRect, NodeId};

impl MotionStore {
    pub(crate) fn spawn_dispatch_receipt(
        &mut self,
        task_ref: impl Into<String>,
        target_node_id: NodeId,
        status: DispatchReceiptVisualStatus,
    ) -> DispatchEffectSpawnOutcome {
        self.spawn_dispatch_receipt_from(
            task_ref,
            DispatchVisualOrigin::CommandDock,
            target_node_id,
            status,
        )
    }

    pub(crate) fn spawn_dispatch_receipt_from(
        &mut self,
        task_ref: impl Into<String>,
        origin: DispatchVisualOrigin,
        target_node_id: NodeId,
        status: DispatchReceiptVisualStatus,
    ) -> DispatchEffectSpawnOutcome {
        if self.dock_flight_suppressed && origin == DispatchVisualOrigin::CommandDock {
            return DispatchEffectSpawnOutcome::Duplicate;
        }
        self.dispatch_receipts.spawn(
            task_ref.into(),
            origin,
            target_node_id,
            status,
            self.reduce_motion,
        )
    }

    pub(crate) fn set_dock_flight_suppressed(&mut self, suppressed: bool) {
        self.dock_flight_suppressed = suppressed;
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

    pub(crate) fn dispatch_endpoint_node_ids(&self) -> Vec<NodeId> {
        self.dispatch_receipts.endpoint_node_ids()
    }

    pub(crate) fn retain_dispatch_endpoints(&mut self, visible_nodes: &[NodeId]) {
        self.dispatch_receipts
            .retain_nodes(|id| visible_nodes.contains(&id));
    }

    pub(crate) fn record_dispatch_unavailable_skip(&mut self) {
        self.dispatch_receipts.record_unavailable_skip();
    }
}
