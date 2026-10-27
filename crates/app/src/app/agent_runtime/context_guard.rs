use super::*;
use agent::context::ContextOperation;

/// Resolve services on the foreground, then wait for IO without holding an entity
/// borrow. The caller must still revalidate its session immediately before execution.
pub(super) async fn prepare(
    this: &gpui::WeakEntity<MultisoulApp>,
    cx: &mut gpui::AsyncWindowContext,
    command: &AdapterCommand,
    context: &agent::ToolContext,
) -> Result<(), AdapterError> {
    if context.cancellation.is_cancelled() {
        return Err(AdapterError::Cancelled);
    }
    let services = this
        .update_in(cx, |app, _, cx| prepare_services(app, command, cx))
        .map_err(|_| AdapterError::Failed("上下文准备无法访问项目窗口".into()))??;
    for service in services {
        service.snapshot().await?;
    }
    if context.cancellation.is_cancelled() {
        return Err(AdapterError::Cancelled);
    }
    Ok(())
}

pub(crate) fn prepare_services(
    app: &mut MultisoulApp,
    command: &AdapterCommand,
    cx: &mut Context<MultisoulApp>,
) -> Result<Vec<crate::app::shared_context::SharedContext>, AdapterError> {
    if restriction(app, command).is_none() {
        return Ok(Vec::new());
    }
    let source = app.shared_context_service()?;
    let mut services = vec![source.clone()];
    if let Some(target) =
        destination(app, command, cx)?.filter(|target| target.scope() != source.scope())
    {
        services.push(target);
    }
    Ok(services)
}

pub(crate) fn execute_norma_adapter_command_with_source(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
    input_source: agent::AgentInputSource,
) -> Result<AdapterResult, AdapterError> {
    let restriction = restriction(app, &command);
    if let Some((operation, target)) = restriction {
        let service = app.shared_context_service()?;
        let destination = destination(app, &command, cx)?;
        service.execute_with_state(operation, target.as_deref(), |source_state| {
            let action = |states: &[(&str, &agent::context::WorkingState)]| {
                // Idea text lives in storage, so it is decorated where it is read.
                if let AdapterCommand::AssignIdea { idea_id, target } = command {
                    return super::idea_commands::assign_idea_with_states(
                        app,
                        window,
                        cx,
                        &idea_id,
                        input_source,
                        super::idea_commands::IdeaDelivery {
                            target,
                            flight: super::idea_commands::IdeaFlight {
                                origin: super::idea_commands::drawer_origin(app, cx),
                                user_initiated: false,
                            },
                            states,
                        },
                    );
                }
                let command = handoff_command(command, states)?;
                super::execute_norma_adapter_command_unchecked(
                    app,
                    window,
                    cx,
                    command,
                    input_source,
                )
            };
            if let Some(destination) =
                destination.filter(|destination| destination.scope() != service.scope())
            {
                destination.execute_with_state(operation, target.as_deref(), |target_state| {
                    action(&[("source", source_state), ("destination", target_state)])
                })
            } else {
                action(&[("source_and_destination", source_state)])
            }
        })
    } else {
        super::execute_norma_adapter_command_unchecked(app, window, cx, command, input_source)
    }
}

// Only decorate at the final host boundary. Outer operation fingerprints and
// ToolContext retain the original user input, so retries keep their existing identity.
fn handoff_command(
    mut command: AdapterCommand,
    states: &[(&str, &agent::context::WorkingState)],
) -> Result<AdapterCommand, AdapterError> {
    let text = match &mut command {
        AdapterCommand::AssignAgentTask { task, .. }
        | AdapterCommand::AssignWorkspaceTask { task, .. }
        | AdapterCommand::CreateAgent {
            task: Some(task), ..
        } => Some(task),
        AdapterCommand::ReplyAgent { message, .. } => Some(message),
        _ => None,
    };
    if let Some(text) = text {
        *text = agent::context::context_task_handoff(text, states)
            .map_err(|error| AdapterError::Rejected(error.into()))?;
    }
    Ok(command)
}

/// Resolve the actual destination before taking either execution lock. Same-area calls
/// must not re-enter a GPUI entity or lock the same coordinator twice.
fn destination(
    app: &mut MultisoulApp,
    command: &AdapterCommand,
    cx: &mut Context<MultisoulApp>,
) -> Result<Option<crate::app::shared_context::SharedContext>, AdapterError> {
    let (workspace_id, area_id) = match command {
        AdapterCommand::AssignWorkspaceTask {
            workspace_id,
            area_id,
            ..
        }
        | AdapterCommand::AssignIdea {
            target:
                agent::IdeaTarget::Agent {
                    workspace_id,
                    area_id,
                    ..
                },
            ..
        }
        | AdapterCommand::AssignIdea {
            target:
                agent::IdeaTarget::NewAgent {
                    workspace_id,
                    area_id,
                    ..
                },
            ..
        } => (workspace_id, area_id),
        _ => return Ok(None),
    };
    let source = app.shared_context_service()?;
    if &source.scope().workspace_id == workspace_id && &source.scope().area_id == area_id {
        return Ok(Some(source));
    }
    let key = (workspace_id.clone(), area_id.clone());
    let session = if let Some(host) = app.desktop_host.as_ref().and_then(|host| host.upgrade()) {
        host.read(cx).desktop_workspace.sessions.get(&key).cloned()
    } else {
        app.desktop_workspace.sessions.get(&key).cloned()
    }
    .ok_or_else(|| AdapterError::Rejected("context_destination_unavailable".into()))?;
    let service = session.update(cx, |app, _| app.shared_context_service())?;
    if &service.scope().workspace_id != workspace_id || &service.scope().area_id != area_id {
        return Err(AdapterError::Rejected("context_destination_changed".into()));
    }
    Ok(Some(service))
}

fn restriction(
    app: &MultisoulApp,
    command: &AdapterCommand,
) -> Option<(ContextOperation, Option<String>)> {
    use AdapterCommand::*;
    let deleting =
        |id: Option<u64>| Some((ContextOperation::DeleteCard, id.map(|id| id.to_string())));
    match command {
        DeleteCanvasCard { node_id } => deleting(Some(*node_id)),
        DeleteCanvasCardAtPosition { position } => deleting(
            position
                .checked_sub(1)
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| app.runtime.document.nodes().get(n))
                .map(|node| node.id),
        ),
        DeleteAgentCard { agent } => deleting(
            app.agent_definitions
                .definition_by_address(agent)
                .and_then(|definition| {
                    app.agent_roster_for_provider()
                        .into_iter()
                        .find(|entry| entry.definition_id == definition.id)
                        .and_then(|entry| entry.active_node_id)
                }),
        ),
        SendTerminalInput { node_id, .. } => Some((
            ContextOperation::SendTerminalInput,
            Some(node_id.to_string()),
        )),
        AssignAgentTask { .. }
        | AssignWorkspaceTask { .. }
        | AssignIdea { .. }
        | CreateAgent { task: Some(_), .. }
        | ReplyAgent { .. } => Some((ContextOperation::DispatchTask, None)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/context_dispatch_handoff_tests.rs"]
mod handoff_tests;
