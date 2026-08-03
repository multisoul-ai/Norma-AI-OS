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
        let _ = spawn_agent_handoff(
            app,
            receipt.task_id.clone(),
            receipt.node_id,
            status,
            visible_area,
        );
    }
}

pub(super) fn spawn_accepted_session_reply(
    app: &mut MultisoulApp,
    reply_id: Option<u64>,
    node_id: Option<u64>,
    reply_status: Option<&str>,
    visible_area: CanvasRect,
) -> bool {
    let (Some(reply_id), Some(node_id)) = (reply_id, node_id) else {
        return false;
    };
    let status = match reply_status {
        Some("sent") => DispatchReceiptVisualStatus::Submitted,
        Some("queued") => DispatchReceiptVisualStatus::Queued,
        _ => return false,
    };
    spawn_agent_handoff(
        app,
        format!("voice-reply:{reply_id}"),
        node_id,
        status,
        visible_area,
    )
}

fn spawn_agent_handoff(
    app: &mut MultisoulApp,
    receipt_ref: String,
    node_id: u64,
    status: DispatchReceiptVisualStatus,
    visible_area: CanvasRect,
) -> bool {
    let target_is_visible = matches!(app.screen, AppScreen::Canvas)
        && app
            .runtime
            .document
            .node(node_id)
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
            receipt_ref,
            target_node_id = node_id,
            "skipped Agent handoff effect outside a visible Canvas target"
        );
        return false;
    }
    let outcome = app
        .motion
        .spawn_dispatch_receipt(receipt_ref.clone(), node_id, status);
    if outcome == DispatchEffectSpawnOutcome::DroppedCapacity {
        tracing::debug!(
            receipt_ref,
            target_node_id = node_id,
            "skipped Agent handoff effect at bounded capacity"
        );
    }
    matches!(
        outcome,
        DispatchEffectSpawnOutcome::Spawned | DispatchEffectSpawnOutcome::Queued
    )
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
