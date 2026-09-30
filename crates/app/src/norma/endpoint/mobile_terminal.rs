use super::NormaEndpointHost;
use crate::mobile::LiveTerminalRequest;
use gpui::App;
use std::{path::Path, time::Instant};

pub(crate) fn read(database: &Path, call: LiveTerminalRequest, cx: &mut App) {
    let target = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| {
            host.routes
                .resolve(&call.request.workspace_id, &call.request.route_id)
                .ok()
        })
        .cloned();
    let result = target
        .and_then(|target| {
            target
                .app
                .update(cx, |app, _| {
                    if app.database_paths.app_db_path() != database
                        || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                        || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                    {
                        return Err("route_expired");
                    }
                    if Instant::now() >= call.deadline {
                        return Err("unavailable");
                    }
                    if let Some(task) = &call.task {
                        validate_task(app, task, call.control.is_none())?;
                        if call.request.node_id != task.key.node_id
                            || call.request.run_id != task.key.run_id
                            || call.request.session_id.as_deref() != Some(&task.key.session_id)
                        {
                            return Err("session_changed");
                        }
                        if let Some(lease) = &call.control {
                            app.runtime.renew_mobile_terminal_input(&task.key, lease)?;
                            return app.runtime.controlled_mobile_terminal_page(
                                &task.key,
                                lease,
                                &call.request,
                            );
                        }
                    }
                    app.runtime.mobile_terminal_page(&call.request)
                })
                .ok()
        })
        .unwrap_or(Err("route_expired"));
    let _ = call.reply.try_send(result);
}

pub(super) fn validate_task(
    app: &crate::MultisoulApp,
    task: &crate::mobile::LiveTerminalTask,
    exact: bool,
) -> Result<(), &'static str> {
    let key = &task.key;
    if key.validate().is_err()
        || (exact && app.runtime.mobile_latest_turn_key(key.node_id).as_ref() != Some(key))
    {
        return Err("session_changed");
    }
    let definition = app
        .runtime
        .document
        .node(key.node_id)
        .and_then(|node| match &node.kind {
            canvas::CanvasNodeKind::Agent(spec) => spec.definition_id,
            _ => None,
        });
    if definition != Some(task.agent_definition_id)
        || app
            .active_orchestration_task_ref(agent::AgentDispatchSource::agent(
                task.agent_definition_id,
                key.node_id,
            ))
            .is_some_and(|active| active != task.task_ref)
    {
        return Err("session_changed");
    }
    Ok(())
}

pub(super) fn control(
    app: &mut crate::MultisoulApp,
    command: &crate::mobile::ThreadTerminalCommand,
) -> Result<serde_json::Value, &'static str> {
    use crate::mobile::ThreadTerminalOperation;
    let key = &command.target.key;
    validate_task(
        app,
        &command.target,
        matches!(command.operation, ThreadTerminalOperation::Claim { .. }),
    )?;
    let status = match &command.operation {
        ThreadTerminalOperation::Claim { .. } => {
            app.runtime
                .claim_mobile_terminal_input(key, &command.lease)?;
            "controlled"
        }
        ThreadTerminalOperation::Input {
            expected,
            screen_signature,
            input_sequence,
            input,
            ..
        } => {
            app.runtime.send_controlled_terminal_input(
                key,
                &command.lease,
                expected,
                *screen_signature,
                *input_sequence,
                input,
            )?;
            "written"
        }
        ThreadTerminalOperation::Release { .. } => {
            app.runtime
                .release_mobile_terminal_input(key, &command.lease)?;
            "released"
        }
        ThreadTerminalOperation::Read { .. } => return Err("invalid_request"),
    };
    Ok(serde_json::json!({"status":status, "task_ref":command.target.task_ref, "key":key}))
}
