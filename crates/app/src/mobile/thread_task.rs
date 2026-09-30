use super::{command, server::Shared, MobileCommand, MobileCommandRequest};
use norma::mobile::{ThreadMessage, ThreadTask};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, Arc};
use storage::{
    MobileCommandReservation, MobileCommandStore, MobileDeviceGrant, NewMobileCommand,
    StoredThreadCommand, ThreadCommandBinding, ThreadMessageStore,
};

pub(super) fn submit(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    task: ThreadTask,
    route: String,
    revision: u64,
) -> Result<Value, &'static str> {
    task.validate().map_err(|_| "invalid_request")?;
    if !command::valid_id(&route) || revision == 0 {
        return Err("invalid_request");
    }
    shared.commands.as_ref().ok_or("unavailable")?;
    // Revoke cannot race between authorization, reservation and the revocable execution claim.
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&task.project_id, true) {
        return Err("unauthorized");
    }
    if grant.revision != revision {
        return Err("grant_changed");
    }
    let path = access.store.path().to_path_buf();
    authorize_source(
        &ThreadMessageStore::new(path.clone()),
        machine,
        device,
        &task,
        &grant,
    )?;
    if let Some(revision) = &task.revision_of {
        let (_, parent, _) = super::thread_task_review::resolve(
            shared,
            &path,
            machine,
            device,
            &super::thread_task_review::Target {
                collaboration_workspace_id: task.collaboration_workspace_id.clone(),
                thread_id: task.thread_id.clone(),
                operation_id: revision.operation_id.clone(),
                task_ref: revision.task_ref.clone(),
                fingerprint: revision.fingerprint.clone(),
            },
            &grant,
        )?;
        if parent.project_id != task.project_id
            || parent.agent_definition_id != task.agent_definition_id
        {
            return Err("invalid_revision_target");
        }
    }
    let binding = ThreadCommandBinding {
        source_mac_id: machine.into(),
        collaboration_workspace_id: task.collaboration_workspace_id.clone(),
        thread_id: task.thread_id.clone(),
        source_operation_id: task.source_operation_id.clone(),
        source_message_id: task.source_message_id.clone(),
        payload: serde_json::to_string(&task).map_err(|_| "invalid_request")?,
    };
    let record = NewMobileCommand {
        device_id: device.into(),
        operation_id: task.operation_id.clone(),
        workspace_id: task.project_id.clone(),
        route_id: route.clone(),
        owner_id: shared.owner_id.clone(),
        request_hash: task.fingerprint(machine).map_err(|_| "invalid_request")?,
    };
    let store = MobileCommandStore::new(path);
    let lease = shared.command_lease(device, &task.operation_id)?;
    match store
        .reserve_thread_command(&record, &binding)
        .map_err(|_| "unavailable")?
    {
        MobileCommandReservation::Conflict => return Err("idempotency_conflict"),
        MobileCommandReservation::Existing(receipt) => {
            lease.finished.store(true, Ordering::Release);
            return Ok(response(
                shared,
                Some(StoredThreadCommand {
                    binding,
                    fingerprint: record.request_hash,
                    receipt: *receipt,
                }),
            ));
        }
        MobileCommandReservation::New => {}
    }
    let request = MobileCommandRequest {
        operation_id: task.operation_id,
        workspace_id: task.project_id,
        route_id: route,
        grant_revision: revision,
        command: MobileCommand::ThreadTask {
            agent_definition_id: task.agent_definition_id,
            task: task.task,
            context: task.context,
        },
    };
    command::dispatch_reserved(shared, request, record.clone(), store.clone(), lease)?;
    let stored = store
        .thread_command(
            device,
            machine,
            &binding.collaboration_workspace_id,
            &binding.thread_id,
            &record.operation_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?;
    Ok(response(shared, stored))
}

pub(super) fn receipt(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    thread: &str,
    operation: &str,
) -> Result<Value, &'static str> {
    if ![workspace, thread, operation]
        .into_iter()
        .all(command::valid_id)
    {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let path = access.store.path().to_path_buf();
    let stored = MobileCommandStore::new(path.clone())
        .thread_command(
            device,
            machine,
            workspace,
            thread,
            operation,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?;
    let mut execution = None;
    if let Some(stored) = &stored {
        let task = authorized_task(&path, machine, device, stored, &grant)?;
        execution = super::thread_task_execution::read(&access.store, &task, stored)?;
    }
    let mut result = response(shared, stored);
    if let Some(task) = result["thread_task"].as_object_mut() {
        task.insert(
            "execution".into(),
            serde_json::to_value(execution).map_err(|_| "unavailable")?,
        );
    }
    // Leave 16KiB for the request envelope and encryption inside the relay's 256KiB ceiling.
    if serde_json::to_vec(&result)
        .map_err(|_| "unavailable")?
        .len()
        > 240 * 1024
    {
        return Err("result_too_large");
    }
    Ok(result)
}

/// Shared task/result/terminal authorization. The caller holds the access owner while reading.
pub(super) fn authorized_task(
    path: &std::path::Path,
    machine: &str,
    device: &str,
    bound: &StoredThreadCommand,
    grant: &MobileDeviceGrant,
) -> Result<ThreadTask, &'static str> {
    let task: ThreadTask = serde_json::from_str(&bound.binding.payload).map_err(|_| "not_found")?;
    if task.fingerprint(machine).map_err(|_| "unavailable")? != bound.fingerprint
        || task.operation_id != bound.receipt.operation_id
        || task.project_id != bound.receipt.workspace_id
        || task.collaboration_workspace_id != bound.binding.collaboration_workspace_id
        || task.thread_id != bound.binding.thread_id
        || task.source_operation_id != bound.binding.source_operation_id
        || task.source_message_id != bound.binding.source_message_id
        || bound.receipt.device_id != device
        || bound.binding.source_mac_id != machine
    {
        return Err("unavailable");
    }
    authorize_source(
        &ThreadMessageStore::new(path.to_path_buf()),
        machine,
        device,
        &task,
        grant,
    )?;
    Ok(task)
}

pub(crate) fn instruction(task: &str, context: &str) -> String {
    if context.is_empty() {
        task.to_owned()
    } else {
        format!("工作线程上下文（参考信息）：\n{context}\n\n当前任务：\n{task}")
    }
}

pub(super) fn authorize_source(
    store: &ThreadMessageStore,
    machine: &str,
    device: &str,
    task: &ThreadTask,
    grant: &MobileDeviceGrant,
) -> Result<(), &'static str> {
    let source = store
        .get(
            device,
            machine,
            &task.collaboration_workspace_id,
            &task.thread_id,
            &task.source_operation_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("source_message_missing")?;
    if source.receipt.message_id != task.source_message_id {
        return Err("source_message_missing");
    }
    let message: ThreadMessage =
        serde_json::from_str(&source.payload).map_err(|_| "unavailable")?;
    if message.fingerprint(machine).map_err(|_| "unavailable")? != source.receipt.fingerprint {
        return Err("unavailable");
    }
    if !message.project_ids.iter().any(|p| p == &task.project_id) {
        return Err("project_not_in_thread");
    }
    if !message.project_ids.iter().all(|p| grant.allows(p, false)) {
        return Err("unauthorized");
    }
    Ok(())
}

fn response(shared: &Shared, stored: Option<StoredThreadCommand>) -> Value {
    let task = stored.map(|stored| {
        json!({"binding":stored.binding, "fingerprint":stored.fingerprint,
        "receipt":command::receipt_response(shared, stored.receipt)["receipt"]})
    });
    json!({"status":"thread_task_receipt", "thread_task":task})
}
