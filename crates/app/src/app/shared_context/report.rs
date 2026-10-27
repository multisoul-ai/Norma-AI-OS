//! A report owns one ordered, background-only delivery log. Dropping an unplayed
//! queued report records Skipped; a real start callback is required for Completed.
use super::*;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub(crate) struct ReportDeliveryHandle(Arc<DeliveryOwner>);
struct DeliveryOwner {
    reference: ContextRef,
    state: Mutex<ReportDelivery>,
    sender: mpsc::UnboundedSender<Write>,
}
enum Write {
    Status(ReportDelivery),
    Flush(oneshot::Sender<Result<(), String>>),
}

impl SharedContext {
    pub(crate) fn report_delivery(
        &self,
        reference: ContextRef,
    ) -> Result<ReportDeliveryHandle, AdapterError> {
        if reference.scope != *self.scope() {
            return Err(AdapterError::Rejected("context_scope_mismatch".into()));
        }
        let runtime =
            super::super::agent_runtime::shared_tokio_runtime().map_err(AdapterError::Failed)?;
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let service = self.clone();
        let report = reference.clone();
        runtime.spawn(async move {
            let mut failure = None;
            while let Some(write) = receiver.recv().await {
                match write {
                    Write::Status(status) => {
                        if failure.is_some() { continue; }
                        let mut retries = 0_u32;
                        loop {
                            let reference = report.clone();
                            // Retain accepted writes even after every UI handle is gone.
                            // Retry the same identity; only validation errors are final.
                            let result = service.run(None, move |access| Ok(access.host_event(
                                &format!("{}:{status:?}", reference.event_id),
                                ContextPayload::Delivery { report: reference.clone(), status },
                                vec![reference.clone()],
                            ))).await;
                            let error = match result {
                                Ok(Ok(_)) => break,
                                Ok(Err(storage::StorageError::InvalidContext(error))) => {
                                    failure = Some(error);
                                    break;
                                }
                                Ok(Err(error)) => error.to_string(),
                                Err(error) => error.to_string(),
                            };
                            if retries == 0 {
                                tracing::warn!(report_id = %report.event_id, %error, "report delivery pending storage recovery");
                            }
                            retries = retries.saturating_add(1);
                            tokio::time::sleep(std::time::Duration::from_millis(25 * u64::from(retries.min(10)))).await;
                        }
                    }
                    Write::Flush(reply) => { let _ = reply.send(failure.clone().map_or(Ok(()), Err)); }
                }
            }
        });
        Ok(ReportDeliveryHandle(Arc::new(DeliveryOwner {
            reference,
            state: Mutex::new(ReportDelivery::Created),
            sender,
        })))
    }
}

impl ReportDeliveryHandle {
    pub(crate) fn reference(&self) -> &ContextRef {
        &self.0.reference
    }

    pub(crate) fn set(&self, status: ReportDelivery) {
        let Ok(mut current) = self.0.state.lock() else {
            return;
        };
        let status = match status {
            // Completion without actual start evidence is a delivery failure.
            ReportDelivery::Completed if *current != ReportDelivery::Playing => {
                ReportDelivery::Failed
            }
            _ => status,
        };
        if report_delivery_is_terminal(*current) || *current == status {
            return;
        }
        if *current == ReportDelivery::Playing
            && matches!(status, ReportDelivery::Queued | ReportDelivery::Sent)
        {
            return;
        }
        *current = status;
        let _ = self.0.sender.send(Write::Status(status));
    }

    pub(crate) fn cancel(&self) {
        let Ok(mut current) = self.0.state.lock() else {
            return;
        };
        cancel(&mut current, &self.0.sender);
    }

    pub(crate) async fn flush(&self) -> Result<(), AdapterError> {
        let (reply, response) = oneshot::channel();
        self.0.sender.send(Write::Flush(reply)).map_err(failed)?;
        // GPUI callers have no Tokio reactor. Timeout releases only the waiter,
        // never the accepted background write.
        super::super::agent_runtime::shared_tokio_runtime()
            .map_err(AdapterError::Failed)?
            .spawn(async move {
                tokio::time::timeout(std::time::Duration::from_secs(2), response)
                    .await
                    .map_err(|_| AdapterError::Failed("context_delivery_pending".into()))?
                    .map_err(failed)?
                    .map_err(AdapterError::Failed)
            })
            .await
            .map_err(failed)?
    }
}

fn cancel(current: &mut ReportDelivery, sender: &mpsc::UnboundedSender<Write>) {
    if report_delivery_is_terminal(*current) {
        return;
    }
    *current = if *current == ReportDelivery::Playing {
        ReportDelivery::Interrupted
    } else {
        ReportDelivery::Skipped
    };
    let _ = sender.send(Write::Status(*current));
}
impl Drop for DeliveryOwner {
    fn drop(&mut self) {
        if let Ok(current) = self.state.get_mut() {
            cancel(current, &self.sender);
        }
    }
}
impl std::fmt::Debug for ReportDeliveryHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ReportDeliveryHandle")
            .field(&self.0.reference)
            .finish()
    }
}
impl PartialEq for ReportDeliveryHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.reference == other.0.reference
    }
}
impl Eq for ReportDeliveryHandle {}

#[cfg(test)]
#[path = "../../../tests/unit/report_delivery_recovery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/report_restart_tests.rs"]
mod restart_tests;
