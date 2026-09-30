use super::*;
use crossbeam_channel::{bounded, Sender, TryRecvError};

pub(super) struct InterruptRequest {
    turn_id: String,
    provider_reply: Option<Receiver<Result<Value, String>>>,
    completion: Option<Sender<Result<Value, String>>>,
}

impl InterruptRequest {
    fn finish(mut self, result: Result<Value, String>) {
        if let Some(sender) = self.completion.take() {
            let _ = sender.try_send(result);
        }
    }
}

impl Drop for InterruptRequest {
    fn drop(&mut self) {
        if let Some(sender) = self.completion.take() {
            let _ = sender.try_send(Err(
                "controlled session closed before interrupt completion".into()
            ));
        }
    }
}

impl Runtime {
    /// The caller must authorize project access first. Receipt resolves on the actual turn result,
    /// not the RPC acknowledgement; a naturally completed turn is reported as completed.
    pub fn interrupt_mobile_codex(
        &mut self,
        node_id: NodeId,
        run_id: &str,
        turn_id: &str,
    ) -> Result<Receiver<Result<Value, String>>, String> {
        self.drain_mobile_codex();
        let host = self
            .mobile_runs
            .get_mut(&node_id)
            .filter(|run| run.run_id.as_deref() == Some(run_id))
            .and_then(|run| run.codex.as_mut())
            .filter(|host| host.turn_id.as_deref() == Some(turn_id))
            .ok_or("controlled Codex turn unavailable")?;
        if host.interrupt.is_some() {
            return Err("interrupt already requested".into());
        }
        let provider_reply = host.connection.request(
            "turn/interrupt",
            json!({"threadId":host.thread_id,"turnId":turn_id}),
        )?;
        let (sender, receipt) = bounded(1);
        host.interrupt = Some(InterruptRequest {
            turn_id: turn_id.into(),
            provider_reply: Some(provider_reply),
            completion: Some(sender),
        });
        // A pending audit receipt must never release an approval after this explicit stop request.
        let cancelled = host
            .approvals
            .cancel_all()
            .and_then(|()| host.audit.record(host.approvals.take_changes()));
        if let Err(error) = cancelled {
            host.control.cancel();
            return Err(error);
        }
        Ok(receipt)
    }

    pub fn mobile_codex_interrupt_pending(&self, node_id: NodeId, run_id: &str) -> bool {
        self.mobile_runs
            .get(&node_id)
            .filter(|run| run.run_id.as_deref() == Some(run_id))
            .and_then(|run| run.codex.as_ref())
            .is_some_and(|host| host.interrupt.is_some())
    }
}

impl CodexHost {
    pub(super) fn observe_interrupt(&mut self, event: &Value) -> Result<(), String> {
        let Some(interrupt) = self.interrupt.as_ref() else {
            return Ok(());
        };
        let params = &event["params"];
        if params["threadId"] != self.thread_id {
            return Ok(());
        }
        match event["method"].as_str() {
            Some("turn/completed") if params["turn"]["id"] == interrupt.turn_id => {
                let result = match params["turn"]["status"].as_str() {
                    Some(status @ ("interrupted" | "completed" | "failed")) => {
                        Ok(json!({"turnId":interrupt.turn_id,"status":status}))
                    }
                    _ => Err("unknown Codex terminal status".into()),
                };
                self.interrupt
                    .take()
                    .expect("pending interrupt")
                    .finish(result);
            }
            Some("turn/started") if params["turn"]["id"] != interrupt.turn_id => {
                self.interrupt
                    .take()
                    .expect("pending interrupt")
                    .finish(Err("turn replaced before interrupt completion".into()));
            }
            _ => self.approvals.cancel_all()?,
        }
        Ok(())
    }

    pub(super) fn poll_interrupt(&mut self) -> Result<(), String> {
        let Some(request) = self.interrupt.as_mut() else {
            return Ok(());
        };
        let Some(reply) = request.provider_reply.as_ref() else {
            return Ok(());
        };
        match reply.try_recv() {
            Ok(Ok(_)) => {
                request.provider_reply = None;
                Ok(())
            }
            Err(TryRecvError::Empty) => Ok(()),
            outcome => {
                let error = match outcome {
                    Ok(Err(error)) => error,
                    _ => "interrupt RPC disconnected".into(),
                };
                self.interrupt
                    .take()
                    .expect("pending interrupt")
                    .finish(Err(error.clone()));
                // Cancellation failed: do not revive requests whose approvals were already revoked.
                Err(error)
            }
        }
    }
}
