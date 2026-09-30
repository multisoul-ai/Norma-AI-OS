use norma::mobile::ThreadTask;
use storage::{
    AgentDispatchTaskState as State, AgentDispatchTracking, AppStore, MobileCommandStore,
    MobileTaskDeliveryStore, ProjectStore, StoredThreadCommand, ThreadTaskExecution,
    ThreadTaskExecutionEvent,
};

/// Background-only durable lookup. The caller holds the authorization owner after checking source
/// and every project grant, so revoke cannot slip between authorization and reading the result.
pub(super) fn read(
    app: &AppStore,
    task: &ThreadTask,
    bound: &StoredThreadCommand,
) -> Result<Option<ThreadTaskExecutionEvent>, &'static str> {
    if bound.receipt.state != "succeeded" {
        return Ok(None);
    }
    let receipt = bound.receipt.result.as_ref().ok_or("unavailable")?;
    let reference = receipt["task_ref"].as_str().ok_or("unavailable")?;
    let Some(project) = app
        .workspace_identity_store()
        .get(&task.project_id)
        .map_err(|_| "unavailable")?
    else {
        return Ok(None);
    };
    // Match the app's existing production/test filename; never guess a phone-supplied path or fall
    // back to a production database when an isolated test database is missing.
    let paths = crate::database::default_database_paths().map_err(|_| "unavailable")?;
    let database = paths.project_db_path(&project.canonical_root);
    let Some(stored) = ProjectStore::new(database)
        .agent_dispatch_task_store()
        .get_by_task_ref(reference)
        .map_err(|_| "unavailable")?
    else {
        return Ok(None);
    };
    if stored.task_ref != reference
        || receipt["task_id"].as_u64() != Some(stored.runtime_task_id)
        || receipt["node_id"].as_u64() != Some(stored.node_id)
        || stored.definition_id != task.agent_definition_id
        || stored.task_text != super::thread_task::instruction(&task.task, &task.context)
    {
        return Err("unavailable");
    }
    let state = match stored.state {
        State::Accepted => "accepted",
        State::Submitted => "submitted",
        State::Queued => "queued",
        State::Untracked => "untracked",
        State::Done => "done",
        State::Failed => "failed",
        State::CancelRequested => "cancel_requested",
        State::Cancelled => "cancelled",
        State::Interrupted => "interrupted",
    };
    let terminal = matches!(
        stored.state,
        State::Done | State::Failed | State::Cancelled | State::Interrupted
    );
    let mut result = terminal.then_some(stored.result).flatten();
    if let Some(reply) = result.as_mut() {
        if let Some(text) = reply.text.as_mut() {
            // 16KiB UTF-8 preview; the complete stored reply is not changed or discarded.
            reply.truncated |= truncate(text, 16 * 1024);
        }
    }
    let mut error = stored.error;
    if let Some(text) = error.as_mut() {
        truncate(text, 4096);
    }
    let delivery = MobileTaskDeliveryStore::new(app.path().to_path_buf())
        .for_command(
            &bound.receipt.device_id,
            &bound.receipt.operation_id,
            &task.project_id,
        )
        .map_err(|_| "unavailable")?;
    let execution = ThreadTaskExecution {
        task_ref: stored.task_ref,
        node_id: stored.node_id,
        agent_definition_id: stored.definition_id,
        state: state.into(),
        tracking: match stored.tracking {
            AgentDispatchTracking::Managed => "managed",
            AgentDispatchTracking::Untracked => "untracked",
        }
        .into(),
        error,
        result,
        delivery,
    };
    MobileCommandStore::new(app.path().to_path_buf())
        .record_thread_execution(bound, &execution)
        .map(Some)
        .map_err(|_| "unavailable")
}

fn truncate(text: &mut String, maximum: usize) -> bool {
    if text.len() <= maximum {
        return false;
    }
    let mut end = maximum;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}
