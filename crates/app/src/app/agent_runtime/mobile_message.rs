use crate::app::MultisoulApp;
use gpui::Context;
use norma::mobile::CardSnapshot;
use serde_json::Value;
use std::time::{Duration, Instant};

impl MultisoulApp {
    pub(crate) fn mobile_node_has_pending_work(&self, node: canvas::NodeId) -> bool {
        let definition = self
            .runtime
            .document
            .node(node)
            .and_then(|node| match &node.kind {
                canvas::CanvasNodeKind::Agent(spec) => spec.definition_id,
                _ => None,
            });
        self.agent_hook_server.has_pending()
            || self.pending_agent_submissions.has_pending_for_node(node)
            || self.session_reply_queue.has_pending_for_node(node)
            || definition.is_some_and(|id| {
                self.agent_task_coordinator
                    .as_ref()
                    .is_some_and(|c| c.is_busy(id) || c.queued_task_count(id) > 0)
            })
    }

    /// The gateway owns durable deduplication and authorization. This prepares only this card's real message.
    pub(crate) fn send_mobile_chat(
        &mut self,
        expected: &CardSnapshot,
        text: String,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<Value, String>>, &'static str> {
        let node = expected.node_id;
        if self.mobile_node_has_pending_work(node) || self.mobile_delivery_pending(node) {
            return Err("state_not_ready");
        }
        self.runtime.validate_mobile_message(expected, &text)?;
        let cold = expected.run_id.is_none();
        let (run_id, startup) = if cold {
            // Only the requested original card starts, on the background executor.
            let launch = self
                .runtime
                .prepare_native_agent_terminal(node, 80, 24)
                .map_err(|_| "start_failed")?;
            let run = launch.run_id().to_owned();
            (
                Some(run),
                Some(
                    cx.background_executor()
                        .spawn(async move { launch.spawn() }),
                ),
            )
        } else {
            (expected.run_id.clone(), None)
        };
        let existing = if cold {
            None
        } else {
            Some(
                self.runtime
                    .send_mobile_message(expected, text.clone())
                    .map_err(|error| {
                        if error == "outcome_unknown" {
                            "outcome_unknown"
                        } else {
                            "send_failed"
                        }
                    })?,
            )
        };
        let (reply, receipt) = async_channel::bounded(1);
        cx.spawn(async move |this, cx| {
            //25s Provider acknowledgement +10s pre-send baseline +5s storage margin =40s gateway budget.
            let deadline = Instant::now() + Duration::from_secs(25);
            if let Some(startup) = startup {
                let result = startup.await;
                let attached = this.update(cx, |app, cx| {
                    app.runtime.complete_native_agent_terminal(
                        node,
                        run_id.as_deref().unwrap_or_default(),
                        result.and_then(|prepared| prepared.into_live()),
                    )?;
                    app.mark_session_dirty();
                    cx.notify();
                    Ok::<_, terminal::PtyError>(())
                });
                if !matches!(attached, Ok(Ok(()))) {
                    let _ = reply.try_send(Err("outcome_unknown".into()));
                    return;
                }
            }
            let mut waiting = existing;
            let outcome = loop {
                if Instant::now() >= deadline {
                    break Err("outcome_unknown".into());
                }
                if let Some(waiting) = &waiting {
                    match waiting.try_recv() {
                        Ok(result) => break result,
                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                            break Err("outcome_unknown".into())
                        }
                        Err(crossbeam_channel::TryRecvError::Empty) => {}
                    }
                } else {
                    let step = this
                        .update(cx, |app, cx| -> Result<Option<Value>, String> {
                            let current = app
                                .runtime
                                .mobile_card_snapshots()
                                .into_iter()
                                .find(|c| c.node_id == node)
                                .ok_or("outcome_unknown")?;
                            // Codex can acquire its provider session only after the first input.
                            // The reserved run ID remains the authority throughout startup and acknowledgement.
                            if current.run_id != run_id {
                                return Err("outcome_unknown".into());
                            }
                            if app.mobile_node_has_pending_work(node)
                                || matches!(current.state.as_str(), "unknown" | "preparing")
                            {
                                return Ok(None);
                            }
                            match app.runtime.send_mobile_message(&current, text.clone()) {
                                Ok(receipt) => waiting = Some(receipt),
                                Err(error) if error == "terminal_not_ready" => return Ok(None),
                                Err(error) => return Err(error),
                            }
                            cx.notify();
                            Ok(None)
                        })
                        .unwrap_or_else(|_| Err("outcome_unknown".into()));
                    match step {
                        Ok(Some(result)) => break Ok(result),
                        Err(error) => break Err(error),
                        Ok(None) => {}
                    }
                }
                // 25ms = 40 in-memory checks/s only during one bounded send; no IO on the foreground.
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
            };
            if let Err(error) = &outcome {
                tracing::warn!(%error, node_id=node, "mobile message outcome unknown");
            }
            let _ = reply.try_send(outcome);
        })
        .detach();
        cx.notify();
        Ok(receipt)
    }
}
