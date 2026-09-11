//! Authenticated queue APIs. Queue waiting lives on the Mac, never on a phone's foreground timer.
use super::{
    command::{receipt_response, valid_id},
    server::Shared,
    MobileCommandRequest,
};
use norma::mobile::CardSnapshot;
use serde_json::{json, Value};
use std::sync::Arc;
use storage::{
    MobileCommandReservation, MobileCommandStore, NewMobileCommand, NewQueuedMessage, QueuedMessage,
};

mod dispatch;
pub(super) use dispatch::start;

pub(super) fn enqueue(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    request: MobileCommandRequest,
    record: NewMobileCommand,
) -> Result<Value, &'static str> {
    let super::MobileCommand::QueueMessage { expected, text } = &request.command else {
        return Err("invalid_request");
    };
    let message = NewQueuedMessage {
        node_id: expected.node_id,
        profile: expected.profile.clone(),
        run_id: expected.run_id.clone().ok_or("invalid_request")?,
        session_id: expected.session_id.clone().ok_or("invalid_request")?,
        grant_revision: request.grant_revision,
        text: text.clone(),
    };
    let store = {
        let access = shared.access().map_err(|_| "unavailable")?;
        MobileCommandStore::new(access.store.path().to_path_buf())
    };
    // An old acknowledgement is immutable history; retries do not require the old window to be open.
    if store
        .get(
            device,
            &record.operation_id,
            &record.workspace_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .is_none()
    {
        live_target(shared, machine, device, token, &request, expected)?;
    }
    // Recheck after waiting for the UI, and hold the grant lock until durable acceptance.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&record.workspace_id, true) {
        return Err("unauthorized");
    }
    if grant.revision != request.grant_revision {
        return Err("grant_changed");
    }
    match store
        .enqueue_message(&record, &message)
        .map_err(|_| "queue_unavailable")?
    {
        MobileCommandReservation::Conflict => return Err("idempotency_conflict"),
        MobileCommandReservation::Existing(receipt) => {
            return Ok(receipt_response(shared, *receipt))
        }
        MobileCommandReservation::New => {}
    }
    let receipt = store
        .get(
            device,
            &record.operation_id,
            &record.workspace_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    Ok(receipt_response(shared, receipt))
}

pub(super) fn status(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    operation: &str,
    cancel_revision: Option<u64>,
) -> Result<Value, &'static str> {
    if !valid_id(workspace) || !valid_id(operation) {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(workspace, cancel_revision.is_some()) {
        return Err("unauthorized");
    }
    if cancel_revision.is_some_and(|revision| revision != grant.revision) {
        return Err("grant_changed");
    }
    let store = MobileCommandStore::new(access.store.path().to_path_buf());
    let mut entry = store
        .queued_message(device, operation, workspace, &shared.owner_id)
        .map_err(|_| "unavailable")?
        .ok_or("not_found")?;
    if cancel_revision.is_some() && entry.state != "cancelled" {
        if !store
            .cancel_queued_message(&entry.request)
            .map_err(|_| "unavailable")?
        {
            return Err("queue_not_cancellable");
        }
        super::project_events::changed();
        entry = store
            .queued_message(device, operation, workspace, &shared.owner_id)
            .map_err(|_| "unavailable")?
            .ok_or("not_found")?;
    }
    Ok(json!({"status":"queued_message","queued_message":response(entry)}))
}

pub(super) fn response(entry: QueuedMessage) -> Value {
    json!({"device_id":entry.request.device_id,"operation_id":entry.request.operation_id,
        "workspace_id":entry.request.workspace_id,"route_id":entry.request.route_id,
        "node_id":entry.message.node_id,"profile":entry.message.profile,"run_id":entry.message.run_id,
        "session_id":entry.message.session_id,"text":entry.message.text,
        "state":entry.state,"result":entry.result,"error":entry.error})
}

pub(super) fn live_target(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: &MobileCommandRequest,
    expected: &CardSnapshot,
) -> Result<CardSnapshot, &'static str> {
    let snapshot =
        super::snapshot::read(shared, machine, device, token, Some(&request.workspace_id))?;
    let project = &snapshot["projects"][0];
    if project["route_id"].as_str() != Some(&request.route_id) {
        return Err("route_expired");
    }
    let current: CardSnapshot = project["cards"]
        .as_array()
        .and_then(|cards| {
            cards
                .iter()
                .find(|c| c["node_id"].as_u64() == Some(expected.node_id))
        })
        .cloned()
        .and_then(|card| serde_json::from_value(card).ok())
        .ok_or("not_found")?;
    if !current.mobile_control {
        return Err("mobile_control_required");
    }
    if current.profile != expected.profile
        || current.run_id != expected.run_id
        || current.session_id != expected.session_id
    {
        return Err("card_changed");
    }
    if matches!(current.state.as_str(), "unknown" | "preparing") {
        return Err("state_not_ready");
    }
    Ok(current)
}
