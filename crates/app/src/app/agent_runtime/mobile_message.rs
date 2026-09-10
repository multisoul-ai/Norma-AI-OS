use crate::app::MultisoulApp;
use gpui::Context;
use norma::mobile::CardSnapshot;
use serde_json::{json, Value};
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
        let codex = expected.profile == "Codex";
        let startup = if cold {
            Some(
                if codex {
                    self.start_mobile_codex(node, text.clone(), cx)
                } else {
                    self.start_mobile_agent(node, cx)
                }
                .map_err(|_| "start_failed")?,
            )
        } else {
            None
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
        let previous_session = expected.session_id.clone();
        cx.spawn(async move |this, cx| {
            //25s Provider acknowledgement +10s pre-send baseline +5s storage margin =40s gateway budget.
            let deadline = Instant::now() + Duration::from_secs(25);
            let session = if let Some(startup) = startup {
                match startup.recv().await {
                    Ok(Ok(session)) => Some(session),
                    failure => { tracing::warn!(?failure, node_id=node, "mobile Agent startup was not confirmed"); let _ = reply.try_send(Err("outcome_unknown".into())); return; }
                }
            } else { previous_session };
            let mut waiting = existing;
            let outcome = loop {
                if Instant::now() >= deadline { break Err("outcome_unknown".into()); }
                if let Some(waiting) = &waiting {
                    match waiting.try_recv() {
                        Ok(result) => break result,
                        Err(crossbeam_channel::TryRecvError::Disconnected) => break Err("outcome_unknown".into()),
                        Err(crossbeam_channel::TryRecvError::Empty) => {}
                    }
                } else {
                    let step = this.update(cx, |app, cx| -> Result<Option<Value>, String> {
                        if app.runtime.agent_session_id(node) != session { return Err("outcome_unknown".into()); }
                        let current = app.runtime.mobile_card_snapshots().into_iter().find(|c| c.node_id == node).ok_or("outcome_unknown")?;
                        if current.run_id.is_none() { return Err("outcome_unknown".into()); }
                        if codex {
                            let turn = app.runtime.mobile_codex_initial_turn(node, session.as_deref().ok_or("outcome_unknown")?).ok_or("outcome_unknown")?;
                            return Ok(Some(json!({"node_id":node,"run_id":current.run_id,"session_id":session,"turn_id":turn})));
                        }
                        if app.mobile_node_has_pending_work(node) || current.state == "unknown" { return Ok(None); }
                        match app.runtime.send_mobile_message(&current, text.clone()) {
                            Ok(receipt) => waiting = Some(receipt),
                            Err(error) if error == "terminal_not_ready" => return Ok(None),
                            Err(error) => return Err(error),
                        }
                        cx.notify(); Ok(None)
                    }).unwrap_or_else(|_| Err("outcome_unknown".into()));
                    match step { Ok(Some(result)) => break Ok(result), Err(error) => break Err(error), Ok(None) => {} }
                }
                // 25ms = 40 in-memory checks/s only during one bounded send; no IO on the foreground.
                cx.background_executor().timer(Duration::from_millis(25)).await;
            };
            if let Err(error) = &outcome { tracing::warn!(%error, node_id=node, "mobile message outcome unknown"); }
            let _ = reply.try_send(outcome);
        }).detach();
        cx.notify();
        Ok(receipt)
    }
}
