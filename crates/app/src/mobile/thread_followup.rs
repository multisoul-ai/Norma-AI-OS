use super::{command, queue, server::Shared, terminal, thread_task_terminal as original};
use norma::mobile::{ThreadFollowup, ThreadMessage};
use serde_json::{json, Value};
use storage::{
    MobileCommandReservation, MobileCommandStore, MobileTaskDeliveryStore, MobileTurnStore,
    NewMobileCommand, NewQueuedMessage, ThreadCommandBinding, ThreadMessageStore,
};

/// Authentication and durable enqueue stay on the background transport owner. The existing queue
/// rechecks the exact original session before every input; reads never enqueue or retarget it.
pub(super) fn handle(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    followup: &ThreadFollowup,
    attempt: Option<(&str, u64)>,
) -> Result<Value, &'static str> {
    followup.validate().map_err(|_| "invalid_request")?;
    if attempt.is_some_and(|(route, revision)| !command::valid_id(route) || revision == 0) {
        return Err("invalid_request");
    }
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
    let record = NewMobileCommand {
        device_id: device.into(),
        operation_id: followup.operation_id.clone(),
        workspace_id: scope.workspace_id.clone(),
        route_id: scope.route_id.clone(),
        owner_id: shared.owner_id.clone(),
        request_hash: fingerprint.clone(),
    };
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
    if existing.is_none() {
        if let Some((route, revision)) = attempt {
            if shared.commands.is_none() || shared.snapshots.is_none() || shared.terminals.is_none()
            {
                return Err("unavailable");
            }
            if revision != resolved_revision {
                return Err("grant_changed");
            }
            if route != scope.route_id {
                return Err("route_expired");
            }
            // The foreground verifies task_ref, Agent definition and original run/session. Reading
            // under terminal control is allowed: input waits in the shared queue until handed back.
            let current =
                continuation_target(&path, &store, &record, &binding, followup, target.clone())?;
            terminal::read_task(shared, scope.clone(), Some(current))?;
        }
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
    if !grant.allows(&scope.workspace_id, attempt.is_some()) {
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
    if let Some((_, revision)) = attempt {
        if revision != grant.revision {
            return Err("grant_changed");
        }
        if existing.is_none() {
            let message = NewQueuedMessage {
                node_id: target.key.node_id,
                profile: target.key.profile.clone(),
                run_id: target.key.run_id.clone(),
                session_id: target.key.session_id.clone(),
                grant_revision: revision,
                text: followup.text.clone(),
            };
            if matches!(
                store
                    .enqueue_thread_message(&record, &binding, &message)
                    .map_err(|_| "queue_unavailable")?,
                MobileCommandReservation::Conflict
            ) {
                return Err("idempotency_conflict");
            }
        }
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

/// Recover the immutable parent at each dequeue. The App must still validate this exact target
/// immediately before input, because a new task may take the node during baseline capture.
pub(super) fn queued_target(
    path: &std::path::Path,
    store: &MobileCommandStore,
    entry: &storage::QueuedMessage,
    grant: &storage::MobileDeviceGrant,
) -> Result<Option<super::LiveTerminalTask>, &'static str> {
    let Some(binding) = store
        .queued_thread_binding(&entry.request)
        .map_err(|_| "unavailable")?
    else {
        return Ok(None);
    };
    let followup = bound_followup(&entry.request, &binding)?;
    let request = original::Request {
        collaboration_workspace_id: followup.collaboration_workspace_id.clone(),
        thread_id: followup.thread_id.clone(),
        operation_id: followup.parent_operation_id.clone(),
        task_ref: followup.task_ref.clone(),
        before: None,
        revision: None,
    };
    let (scope, target) = original::resolve_authorized(
        path,
        grant,
        &binding.source_mac_id,
        &entry.request.device_id,
        &entry.request.owner_id,
        &request,
    )?;
    if scope.workspace_id != entry.request.workspace_id
        || scope.route_id != entry.request.route_id
        || target.key.node_id != entry.message.node_id
        || target.key.profile != entry.message.profile
        || target.key.run_id != entry.message.run_id
        || target.key.session_id != entry.message.session_id
        || followup.text != entry.message.text
    {
        return Err("session_changed");
    }
    continuation_target(path, store, &entry.request, &binding, &followup, target).map(Some)
}

fn bound_followup(
    record: &NewMobileCommand,
    binding: &ThreadCommandBinding,
) -> Result<ThreadFollowup, &'static str> {
    let followup: ThreadFollowup =
        serde_json::from_str(&binding.payload).map_err(|_| "unavailable")?;
    if followup
        .fingerprint(&binding.source_mac_id)
        .map_err(|_| "unavailable")?
        != record.request_hash
        || followup.operation_id != record.operation_id
        || followup.collaboration_workspace_id != binding.collaboration_workspace_id
        || followup.thread_id != binding.thread_id
        || followup.source_operation_id != binding.source_operation_id
        || followup.source_message_id != binding.source_message_id
    {
        return Err("unavailable");
    }
    Ok(followup)
}

fn continuation_target(
    path: &std::path::Path,
    store: &MobileCommandStore,
    record: &NewMobileCommand,
    binding: &ThreadCommandBinding,
    followup: &ThreadFollowup,
    mut target: super::LiveTerminalTask,
) -> Result<super::LiveTerminalTask, &'static str> {
    let Some(previous) = store
        .latest_thread_message(
            record,
            binding,
            &followup.parent_operation_id,
            &followup.task_ref,
        )
        .map_err(|_| "unavailable")?
    else {
        return Ok(target);
    };
    let previous_binding = store
        .queued_thread_binding(&previous.request)
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    let previous_followup = bound_followup(&previous.request, &previous_binding)?;
    let key = storage::MobileTaskBaselineStore::new(path.to_path_buf())
        .turn_for_command(
            &previous.request.device_id,
            &previous.request.operation_id,
            &previous.request.workspace_id,
            &previous.request.route_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    if previous_followup.parent_operation_id != followup.parent_operation_id
        || previous_followup.task_ref != target.task_ref
        || key.node_id != target.key.node_id
        || key.profile != target.key.profile
        || key.run_id != target.key.run_id
        || key.session_id != target.key.session_id
        || key.turn_id == target.key.turn_id
        || previous.message.text != previous_followup.text
        || previous.message.node_id != key.node_id
        || previous.message.profile != key.profile
        || previous.message.run_id != key.run_id
        || previous.message.session_id != key.session_id
        || previous.result.as_ref().is_none_or(|result| {
            result["node_id"].as_u64() != Some(key.node_id)
                || result["run_id"] != key.run_id
                || result["session_id"] != key.session_id
                || result["turn_id"] != key.turn_id
        })
    {
        return Err("session_changed");
    }
    target.key = key;
    Ok(target)
}
