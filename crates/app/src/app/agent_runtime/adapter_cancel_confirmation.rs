//! Observe an exact Provider turn off the foreground; an acknowledgement is not a terminal result.
use crate::app::MultisoulApp;
use gpui::Context;
use serde_json::Value;
use std::time::Duration;

pub(super) fn watch(
    task: agent::AgentCoordinatedTask,
    card: norma::mobile::CardSnapshot,
    waiting: Option<crossbeam_channel::Receiver<Result<Value, String>>>,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn(async move |this, cx| {
        let deadline = cx.background_executor().now() + Duration::from_secs(30);
        let outcome = loop {
            match waiting
                .as_ref()
                .map(|receiver| receiver.try_recv())
                .unwrap_or(Err(crossbeam_channel::TryRecvError::Empty))
            {
                Ok(outcome) => break outcome,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    break Err("cancel_unconfirmed: Provider disconnected".into())
                }
                Err(crossbeam_channel::TryRecvError::Empty)
                    if cx.background_executor().now() >= deadline =>
                {
                    break Err(
                        "cancel_unconfirmed: Provider did not confirm within 30 seconds".into(),
                    )
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        };
        let _ = this.update(cx, |app, cx| {
            let still_pending = app
                .agent_task_coordinator
                .as_ref()
                .is_some_and(|coordinator| {
                    coordinator
                        .task_by_ref(&task.task_ref)
                        .is_ok_and(|current| {
                            current.state == "cancel_requested"
                                && current.target_node_id == card.node_id
                                && current.definition_id == task.definition_id
                        })
                });
            let same_run = app
                .runtime
                .mobile_card_snapshots()
                .into_iter()
                .any(|current| {
                    current.node_id == card.node_id
                        && current.run_id == card.run_id
                        && current.session_id == card.session_id
                });
            if !still_pending {
                return;
            }
            let confirmed = outcome.as_ref().is_ok_and(|value| {
                value["turnId"].as_str() == card.turn_id.as_deref()
                    && matches!(
                        value["status"].as_str(),
                        Some("interrupted" | "completed" | "failed")
                    )
            });
            if confirmed && same_run {
                app.advance_agent_task_queue(card.node_id);
            } else {
                // Keep the card busy and preserve queued work until an actual terminal Hook arrives.
                // A failed/unconfirmed stop must never submit the next task into a still-running turn.
                if let Some(coordinator) = app.agent_task_coordinator.as_mut() {
                    coordinator.fail_active_preserving_busy(task.definition_id, task.task_id);
                }
                if let Some(writer) = app.persistence.agent_dispatch_writer() {
                    writer.finish_task(
                        task.task_id,
                        storage::AgentDispatchTaskState::Failed,
                        Some(format!(
                            "cancel_unconfirmed: {}",
                            outcome
                                .err()
                                .unwrap_or_else(|| "target or turn changed".into())
                        )),
                    );
                }
            }
            cx.notify();
        });
    })
    .detach();
}
