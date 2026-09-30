use super::{Action, Request, Shared};
use norma::mobile::ThreadMessage;
use serde_json::{json, Value};
use storage::{MobileDeviceGrant, NewThreadMessage, StoredThreadMessage, ThreadMessageStore};

pub(super) fn handle(
    shared: &Shared,
    request: &Request,
    device: &str,
    token: &str,
) -> Result<Value, &'static str> {
    // Keep the auth lock through the transaction so a completed revoke cannot race a new receipt.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(&request.machine_id, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let store = ThreadMessageStore::new(access.store.path().to_path_buf());
    match &request.action {
        Action::ThreadMessage { message, .. } => {
            message.validate().map_err(|_| "invalid_request")?;
            authorize(message, &grant)?;
            let receipt = store
                .receive(&NewThreadMessage {
                    device_id: device.into(),
                    source_mac_id: request.machine_id.clone(),
                    collaboration_workspace_id: message.collaboration_workspace_id.clone(),
                    thread_id: message.thread_id.clone(),
                    operation_id: message.operation_id.clone(),
                    message_id: message.message_id.clone(),
                    origin_device_id: message.origin_device_id.clone(),
                    fingerprint: message
                        .fingerprint(&request.machine_id)
                        .map_err(|_| "invalid_request")?,
                    payload: serde_json::to_string(message).map_err(|_| "invalid_request")?,
                })
                .map_err(store_error)?;
            Ok(json!({"status":"thread_receipt", "thread_receipt":receipt}))
        }
        Action::ThreadReceipt {
            collaboration_workspace_id,
            thread_id,
            operation_id,
            ..
        } => {
            let message = store
                .get(
                    device,
                    &request.machine_id,
                    collaboration_workspace_id,
                    thread_id,
                    operation_id,
                )
                .map_err(store_error)?;
            if let Some(message) = &message {
                check_stored(message, &grant)?;
            }
            Ok(json!({"status":"thread_receipt", "thread_receipt":message.map(|m| m.receipt)}))
        }
        Action::ThreadEvents {
            collaboration_workspace_id,
            thread_id,
            after,
            ..
        } => {
            let messages = store
                .events(
                    device,
                    &request.machine_id,
                    collaboration_workspace_id,
                    thread_id,
                    *after,
                )
                .map_err(store_error)?;
            for message in &messages {
                check_stored(message, &grant)?;
            }
            Ok(
                json!({"status":"thread_events", "thread_events":messages.into_iter().map(|m| m.receipt).collect::<Vec<_>>()}),
            )
        }
        _ => Err("invalid_request"),
    }
}

fn authorize(message: &ThreadMessage, grant: &MobileDeviceGrant) -> Result<(), &'static str> {
    if message.project_ids.iter().all(|id| grant.allows(id, false)) {
        Ok(())
    } else {
        Err("unauthorized")
    }
}

fn check_stored(
    stored: &StoredThreadMessage,
    grant: &MobileDeviceGrant,
) -> Result<(), &'static str> {
    let message: ThreadMessage =
        serde_json::from_str(&stored.payload).map_err(|_| "unavailable")?;
    if message
        .fingerprint(&stored.receipt.source_mac_id)
        .map_err(|_| "unavailable")?
        != stored.receipt.fingerprint
    {
        return Err("unavailable");
    }
    authorize(&message, grant)
}

fn store_error(error: storage::StorageError) -> &'static str {
    match error {
        storage::StorageError::InvalidMobileCommand(_) => "idempotency_conflict",
        _ => "unavailable",
    }
}
