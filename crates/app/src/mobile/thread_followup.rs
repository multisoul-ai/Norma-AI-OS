use super::{command, queue, server::Shared, thread_task_terminal as original};
use norma::mobile::{ThreadFollowup, ThreadMessage};
use serde_json::{json, Value};
use storage::{
    MobileCommandStore, MobileTaskDeliveryStore, MobileTurnStore, ThreadCommandBinding,
    ThreadMessageStore,
};

/// Authentication and durable enqueue stay on the background transport owner. The existing queue
/// rechecks the exact original session before every input; reads never enqueue or retarget it.
pub(super) fn handle(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    followup: &ThreadFollowup,
) -> Result<Value, &'static str> {
    followup.validate().map_err(|_| "invalid_request")?;
    let parent = original::Request {
        collaboration_workspace_id: followup.collaboration_workspace_id.clone(),
        thread_id: followup.thread_id.clone(),
        operation_id: followup.parent_operation_id.clone(),
        task_ref: followup.task_ref.clone(),
        before: None,
        revision: None,
    };
    let (scope, target, resolved_revision) =
        original::resolve(shared, machine, device, token, &parent)?;
    let binding = ThreadCommandBinding {
        source_mac_id: machine.into(),
        collaboration_workspace_id: followup.collaboration_workspace_id.clone(),
        thread_id: followup.thread_id.clone(),
        source_operation_id: followup.source_operation_id.clone(),
        source_message_id: followup.source_message_id.clone(),
        payload: serde_json::to_string(followup).map_err(|_| "invalid_request")?,
    };
    let fingerprint = followup
        .fingerprint(machine)
        .map_err(|_| "invalid_request")?;
    let path = {
        let access = shared.access().map_err(|_| "unavailable")?;
        access.store.path().to_path_buf()
    };
    let store = MobileCommandStore::new(path.clone());
    let parent_source = store
        .thread_command(
            device,
            machine,
            &followup.collaboration_workspace_id,
            &followup.thread_id,
            &followup.parent_operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("not_found")?
        .binding;
    if followup.source_operation_id == parent_source.source_operation_id
        || followup.source_message_id == parent_source.source_message_id
    {
        return Err("invalid_request");
    }
    let existing = store
        .thread_command(
            device,
            machine,
            &followup.collaboration_workspace_id,
            &followup.thread_id,
            &followup.operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?;
    if existing
        .as_ref()
        .is_some_and(|saved| saved.binding != binding || saved.fingerprint != fingerprint)
    {
        return Err("idempotency_conflict");
    }
    // Revoke/grant edits cannot race the enqueue transaction; a changed grant after the foreground
    // read invalidates the attempt, including permissions on the original task's shared projects.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if grant.revision != resolved_revision {
        return Err("grant_changed");
    }
    if !grant.allows(&scope.workspace_id, false) {
        return Err("unauthorized");
    }
    let path = access.store.path();
    let source = ThreadMessageStore::new(path.to_path_buf())
        .get(
            device,
            machine,
            &followup.collaboration_workspace_id,
            &followup.thread_id,
            &followup.source_operation_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("source_message_missing")?;
    if source.receipt.message_id != followup.source_message_id {
        return Err("source_message_missing");
    }
    let message: ThreadMessage =
        serde_json::from_str(&source.payload).map_err(|_| "unavailable")?;
    if message.fingerprint(machine).map_err(|_| "unavailable")? != source.receipt.fingerprint {
        return Err("unavailable");
    }
    if message.text != followup.text {
        return Err("source_message_changed");
    }
    if !message.project_ids.contains(&scope.workspace_id) {
        return Err("project_not_in_thread");
    }
    if !message
        .project_ids
        .iter()
        .all(|project| grant.allows(project, false))
    {
        return Err("unauthorized");
    }
    let bound = store
        .thread_command(
            device,
            machine,
            &followup.collaboration_workspace_id,
            &followup.thread_id,
            &followup.operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?;
    let Some(bound) = bound else {
        return Ok(json!({"status":"thread_followup_receipt","thread_followup":null}));
    };
    if bound.binding != binding || bound.fingerprint != fingerprint {
        return Err("idempotency_conflict");
    }
    let queued = store
        .queued_message(
            device,
            &followup.operation_id,
            &scope.workspace_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    if queued.request.route_id != scope.route_id
        || queued.message.node_id != target.key.node_id
        || queued.message.profile != target.key.profile
        || queued.message.run_id != target.key.run_id
        || queued.message.session_id != target.key.session_id
        || queued.message.text != followup.text
    {
        return Err("unavailable");
    }
    let (outcome, delivery) = if queued.state == "sent" {
        let sent = queued.result.as_ref().ok_or("unavailable")?;
        let mut key = target.key.clone();
        key.turn_id = sent["turn_id"].as_str().ok_or("unavailable")?.into();
        key.validate().map_err(|_| "unavailable")?;
        if key.turn_id == target.key.turn_id
            || sent["node_id"].as_u64() != Some(key.node_id)
            || sent["run_id"] != key.run_id
            || sent["session_id"] != key.session_id
        {
            return Err("unavailable");
        }
        let outcome =
            MobileTurnStore::for_workspace(path.to_path_buf(), scope.workspace_id.clone())
                .get(&key)
                .map_err(|_| "unavailable")?;
        if outcome.as_ref().is_some_and(|value| value.key != key) {
            return Err("unavailable");
        }
        let delivery = MobileTaskDeliveryStore::new(path.to_path_buf())
            .for_command(device, &followup.operation_id, &scope.workspace_id)
            .map_err(|_| "unavailable")?;
        if delivery
            .as_ref()
            .is_some_and(|value| value.outcome.key != key)
        {
            return Err("unavailable");
        }
        (outcome, delivery)
    } else {
        (None, None)
    };
    let result = json!({"status":"thread_followup_receipt", "thread_followup":{
        "binding":bound.binding,"fingerprint":bound.fingerprint,"original_key":target.key,
        "receipt":command::receipt_response(shared, bound.receipt)["receipt"],
        "queued_message":queue::response(queued),"outcome":outcome,"delivery":delivery,
    }});
    //256KiB relay frame minus16KiB routing/encryption margin =240KiB response budget.
    if serde_json::to_vec(&result)
        .map_err(|_| "unavailable")?
        .len()
        > 240 * 1024
    {
        return Err("result_too_large");
    }
    Ok(result)
}
