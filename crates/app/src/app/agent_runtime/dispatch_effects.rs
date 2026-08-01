use crate::app::{AppScreen, MultisoulApp};
use crate::motion::{DispatchEffectSpawnOutcome, DispatchReceiptVisualStatus};
use canvas::{CanvasNodeKind, CanvasRect};
use norma::protocol::{AgentTaskReceipt, AgentTaskReceiptStatus};

pub(super) fn spawn_accepted_dispatch_receipts(
    app: &mut MultisoulApp,
    receipts: &[AgentTaskReceipt],
    visible_area: CanvasRect,
) {
    for receipt in receipts {
        let Some(status) = visual_status(receipt.status) else {
            continue;
        };
        let target_is_visible = matches!(app.screen, AppScreen::Canvas)
            && app.window_active
            && app
                .runtime
                .document
                .node(receipt.node_id)
                .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
                .is_some_and(|node| {
                    let rect = app.motion.current_node_rect(node.id, node.bounds);
                    visible_area.contains(canvas::Vec2::new(
                        rect.origin.x + rect.size.x / 2.0,
                        rect.origin.y + rect.size.y / 2.0,
                    ))
                });
        if !target_is_visible {
            app.motion.record_dispatch_unavailable_skip();
            tracing::debug!(
                task_ref = %receipt.task_id,
                target_node_id = receipt.node_id,
                "skipped dispatch receipt effect outside a visible active Canvas"
            );
            continue;
        }
        let outcome =
            app.motion
                .spawn_dispatch_receipt(receipt.task_id.clone(), receipt.node_id, status);
        if outcome == DispatchEffectSpawnOutcome::DroppedCapacity {
            tracing::debug!(
                task_ref = %receipt.task_id,
                target_node_id = receipt.node_id,
                "skipped dispatch receipt effect at bounded capacity"
            );
        }
    }
}

fn visual_status(status: AgentTaskReceiptStatus) -> Option<DispatchReceiptVisualStatus> {
    match status {
        AgentTaskReceiptStatus::Submitted => Some(DispatchReceiptVisualStatus::Submitted),
        AgentTaskReceiptStatus::Queued => Some(DispatchReceiptVisualStatus::Queued),
        AgentTaskReceiptStatus::Untracked => Some(DispatchReceiptVisualStatus::Untracked),
        AgentTaskReceiptStatus::Accepted
        | AgentTaskReceiptStatus::Failed
        | AgentTaskReceiptStatus::Cancelled
        | AgentTaskReceiptStatus::CancelRequested
        | AgentTaskReceiptStatus::Interrupted
        | AgentTaskReceiptStatus::Done => None,
    }
}
