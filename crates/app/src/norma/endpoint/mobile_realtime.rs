use super::mobile_command::Started;
use crate::app::MultisoulApp;
use agent::{AdapterCommand, AgentInputSource};
use gpui::{Context, Window};

/// Execute a validated mobile command through the existing optional-window adapter.
/// The mobile command owner reserves the operation before entering here and persists its result.
pub(super) fn start(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
    source: storage::NewMobileCommand,
    input_source: AgentInputSource,
) -> Result<Started, &'static str> {
    if let AdapterCommand::AssignAgentTask { agent, .. } = &command {
        // Resolve before entering the adapter: a missing target proves no card or task was created.
        // Errors after dispatch starts remain unknown because they may follow partial side effects.
        if app.agent_definitions.definition_by_address(agent).is_none() {
            return Err("agent_not_found");
        }
    }
    let dispatch = matches!(
        command,
        AdapterCommand::CreateAgent { task: Some(_), .. } | AdapterCommand::AssignAgentTask { .. }
    );
    let query = matches!(command, AdapterCommand::FindAgentTasks { .. });
    let result = crate::app::execute_norma_adapter_command_with_source(
        app,
        window,
        cx,
        command,
        input_source,
    )
    // Creation + dispatch can fail after creating a card. Once the adapter starts, an error
    // cannot prove zero side effects; preserve the durable unknown receipt instead of retrying.
    .map_err(|_| "outcome_unknown")?;
    if result.status == "unknown" {
        return Err("outcome_unknown");
    }
    if !query {
        app.mark_canvas_dirty();
        cx.notify();
    }
    let task = result.task_id.and_then(|id| {
        app.agent_task_coordinator
            .as_ref()?
            .tasks()
            .into_iter()
            .find(|task| task.task_id == id)
    });
    if result.task_id.is_some() && task.is_none() {
        return Err("outcome_unknown");
    }
    let mut value = serde_json::to_value(result).map_err(|_| "outcome_unknown")?;
    let Some(task) = task else {
        return Ok(Started::Saved(value, None));
    };
    // Runtime task_id can be reused after restart; the persisted reference cannot.
    value
        .as_object_mut()
        .ok_or("outcome_unknown")?
        .insert("task_ref".into(), task.task_ref.clone().into());
    if dispatch {
        app.pending_agent_submissions
            .track_realtime_delivery(&task, source);
    }
    let writer = app
        .persistence
        .agent_dispatch_writer()
        .cloned()
        .ok_or("outcome_unknown")?;
    let store = app
        .persistence
        .project_store()
        .ok_or("outcome_unknown")?
        .agent_dispatch_task_store();
    let (sender, saved) = async_channel::bounded(1);
    cx.background_executor()
        .spawn(async move {
            // flush is only a barrier: read back the exact durable identity and instruction as proof.
            let verified = writer.flush().is_ok()
                && store
                    .get_by_task_ref(&task.task_ref)
                    .ok()
                    .flatten()
                    .is_some_and(|stored| {
                        stored.runtime_task_id == task.task_id
                            && stored.node_id == task.target_node_id
                            && stored.task_text == task.task
                            && (task.state != "cancelled"
                                || stored.state == storage::AgentDispatchTaskState::Cancelled)
                    });
            let _ = sender.try_send(if verified {
                Ok(())
            } else {
                Err("outcome_unknown")
            });
        })
        .detach();
    Ok(Started::Saved(value, Some(saved)))
}
