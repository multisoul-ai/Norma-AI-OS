use super::{execution, Session, Shared};
use agent::{NormaToolRuntime, ToolContext};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use storage::mobile_command_store::{MobileCommandStore, RealtimeCall, RealtimeReservation};

pub(super) async fn execute(
    shared: Arc<Shared>,
    session: Arc<Session>,
    call: Value,
    context: ToolContext,
) -> Value {
    let identity = RealtimeCall {
        device_id: session.device.clone(),
        call_id: session.binding.call_id.clone(),
        turn_id: context.turn_id.clone(),
        workspace_id: context.project_id.clone(),
        operation_id: context.operation_id.clone().unwrap_or_default(),
        request_hash: hex::encode(Sha256::digest(
            json!([context.project_id, call["name"], call["arguments"]]).to_string(),
        )),
    };
    let owner = Arc::clone(&shared);
    let binding = Arc::clone(&session);
    let record = identity.clone();
    let reserved = tokio::task::spawn_blocking(move || {
        let access = owner.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&binding.machine, &binding.device, &binding.token)
            .map_err(|_| "unavailable")?
            .ok_or("unauthorized")?;
        if !grant.allows(&record.workspace_id, true) || binding.cancelled.is_cancelled() {
            return Err("unauthorized");
        }
        let store = MobileCommandStore::new(access.store.path().to_path_buf());
        drop(access);
        let reservation = store
            .reserve_realtime(&record)
            .map_err(|_| "idempotency_conflict")?;
        crate::mobile::project_events::changed();
        Ok((store, reservation))
    })
    .await;
    let (store, reservation) = match reserved {
        Ok(Ok(value)) => value,
        Ok(Err("idempotency_conflict")) => return rejected("原实时操作身份冲突或记录不可用"),
        Ok(Err(code)) => return rejected(code),
        Err(_) => return unknown(),
    };
    match reservation {
        RealtimeReservation::Existing(value) if value["status"] != "unknown" => return value,
        RealtimeReservation::Existing(_) | RealtimeReservation::Pending => {
            // Recovery can only READ the existing command, never call submit again.
            let record = identity.clone();
            return tokio::task::spawn_blocking(move || {
                let result = crate::mobile::command::receipt(
                    &shared,
                    &session.machine,
                    &session.device,
                    &session.token,
                    &record.workspace_id,
                    &record.operation_id,
                );
                let value = match result {
                    Ok(value) if value["receipt"]["state"] == "succeeded" => {
                        serde_json::from_value::<agent::AdapterResult>(
                            value["receipt"]["result"].clone(),
                        )
                        .ok()
                        .and_then(|result| serde_json::to_value(result).ok())
                        .unwrap_or_else(unknown)
                    }
                    Ok(value) if value["receipt"]["state"] == "rejected" => {
                        rejected(value["receipt"]["error"].as_str().unwrap_or("rejected"))
                    }
                    _ => return unknown(),
                };
                if store.finish_realtime(&record, value.clone()).is_err() {
                    return unknown();
                }
                crate::mobile::project_events::changed();
                value
            })
            .await
            .unwrap_or_else(|_| unknown());
        }
        RealtimeReservation::Blocked(agent::AdapterError::Failed(_)) => return unknown(),
        RealtimeReservation::Blocked(error) => return rejected(&error.to_string()),
        RealtimeReservation::New => {}
    }
    let args = call["arguments"]
        .as_str()
        .filter(|args| args.len() <= 32 * 1024)
        .and_then(|args| serde_json::from_str(args).ok());
    let result = if context.cancellation.is_cancelled() || session.cancelled.is_cancelled() {
        rejected("stale_turn")
    } else if let Some(args) = args {
        let runtime = NormaToolRuntime::new(Arc::new(execution::Tools { shared, session }));
        match runtime
            .execute(call["name"].as_str().unwrap_or(""), args, &context)
            .await
        {
            Ok(result) => serde_json::to_value(result).expect("serializable tool result"),
            Err(error) => rejected(&error.to_string()),
        }
    } else {
        rejected("invalid realtime tool arguments")
    };
    let value = result.clone();
    match tokio::task::spawn_blocking(move || store.finish_realtime(&identity, value)).await {
        Ok(Ok(())) => {
            crate::mobile::project_events::changed();
            result
        }
        _ => unknown(), // Preserve the unfinished reservation if a known result could not be committed.
    }
}

fn rejected(message: &str) -> Value {
    json!({"status":"rejected","message":message})
}
fn unknown() -> Value {
    json!({"status":"unknown","message":"原操作结果待核对，禁止重复派工；请查看原回执"})
}
