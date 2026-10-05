use super::{command, server::Shared};
use norma::mobile::{ThreadMessage, ThreadTask};
use serde_json::{json, Value};
use storage::{MobileCommandStore, MobileDeviceGrant, StoredThreadCommand, ThreadMessageStore};

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
