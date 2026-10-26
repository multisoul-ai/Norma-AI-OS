use super::mobile_command::Started;
use crate::app::MultisoulApp;
use agent::{AdapterCommand, AgentInputSource};
use gpui::{Context, Window};

/// Preparation does not claim the execution lease. execute_now rechecks the route,
/// deadline and revocation immediately before it accepts the actual operation.
pub(super) fn prepare(
    database: std::path::PathBuf,
    call: crate::mobile::LiveCommandRequest,
    cx: &mut gpui::App,
) {
    let target = cx
        .try_global::<super::NormaEndpointHost>()
        .and_then(|host| {
            host.routes
                .resolve(&call.request.workspace_id, &call.request.route_id)
                .ok()
        })
        .cloned();
    let Some(target) = target else {
        let _ = call.reply.try_send(Err("route_expired"));
        return;
    };
    let crate::mobile::MobileCommand::RealtimeTool { command } = &call.request.command else {
        return;
    };
    let prepared = target
        .app
        .update(cx, |app, cx| {
            if app.database_paths.app_db_path() != database
                || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
            {
                return Err("route_expired");
            }
            crate::app::prepare_context_services(app, command, cx)
                .map_err(|_| "context_state_unavailable")
        })
        .unwrap_or(Err("route_expired"));
    let services = match prepared {
        Ok(services) => services,
        Err(code) => {
            let _ = call.reply.try_send(Err(code));
            return;
        }
    };
    cx.spawn(async move |cx| {
        for service in services {
            if service.snapshot().await.is_err() {
                let _ = call.reply.try_send(Err("context_state_unavailable"));
                return;
            }
        }
        let reply = call.reply.clone();
        if cx
            .update(|cx| super::mobile_command::execute_now(database, call, cx))
            .is_err()
        {
            let _ = reply.try_send(Err("route_expired"));
        }
    })
    .detach();
}

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
    .map_err(|error| match error {
        agent::AdapterError::Rejected(code) if code == "context_operation_restricted" => {
            "context_operation_restricted"
        }
        agent::AdapterError::Rejected(code) if code == "context_state_unavailable" => {
            "context_state_unavailable"
        }
        _ => "outcome_unknown",
    })?;
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
