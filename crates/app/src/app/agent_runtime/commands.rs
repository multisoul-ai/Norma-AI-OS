use crate::app::MultisoulApp;
use gpui::Window;
use norma::protocol::{
    AgentSummary, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

pub(super) fn execute_agent_card_command(
    app: &mut MultisoulApp,
    window: &mut Window,
    command: CardCommand,
) -> Result<CardCommandResult, String> {
    let area = app.visible_canvas_rect(window);
    let zoom = app.runtime.document.viewport.zoom;
    if matches!(
        command,
        CardCommand::ListAgents
            | CardCommand::CreateAgent { .. }
            | CardCommand::ActivateAgent { .. }
            | CardCommand::DispatchAgentTasks { .. }
    ) {
        return execute_high_level_agent_command(app, command, area, zoom);
    }
    let execution = crate::norma::server::execute_card_command_for_area(
        &mut app.runtime,
        &mut app.motion,
        &mut app.master_layout,
        command,
        area,
        zoom,
    )
    .map_err(|error| error.to_string())?;
    let result = execution.result.clone();
    app.apply_voice_execution_effects(execution);
    Ok(result)
}

fn execute_high_level_agent_command(
    app: &mut MultisoulApp,
    command: CardCommand,
    area: canvas::CanvasRect,
    zoom: f32,
) -> Result<CardCommandResult, String> {
    match command {
        CardCommand::ListAgents => Ok(CardCommandResult::AgentsListed {
            agents: app
                .agent_roster_for_provider()
                .into_iter()
                .map(|entry| AgentSummary {
                    definition_id: entry.definition_id,
                    name: entry.name,
                    speech_alias: entry.speech_alias,
                    profile: entry.profile,
                    enabled: entry.enabled,
                    active_node_id: entry.active_node_id,
                    busy: entry.busy,
                })
                .collect(),
        }),
        CardCommand::CreateAgent {
            name,
            profile,
            activate,
        } => {
            let definition = app
                .agent_definitions
                .create_definition(name, canvas_agent_profile(profile))?;
            let node_id = if activate {
                Some(activate_definition(app, &definition, area, zoom)?.0)
            } else {
                None
            };
            Ok(CardCommandResult::AgentCreated {
                definition_id: definition.id,
                name: definition.name,
                node_id,
            })
        }
        CardCommand::ActivateAgent { name } => {
            let definition = app
                .agent_definitions
                .definition_by_address(&name)
                .cloned()
                .ok_or_else(|| format!("Unknown Agent: {name}"))?;
            if !definition.enabled {
                return Err(format!("Agent {} is disabled", definition.name));
            }
            let (node_id, reused) = activate_definition(app, &definition, area, zoom)?;
            Ok(CardCommandResult::AgentActivated {
                definition_id: definition.id,
                name: definition.name,
                node_id,
                reused,
            })
        }
        CardCommand::DispatchAgentTasks {
            assignments,
            force_untracked,
        } => {
            let definitions = assignments
                .iter()
                .map(|assignment| {
                    let definition = app
                        .agent_definitions
                        .definition_by_address(&assignment.name)
                        .cloned()
                        .ok_or_else(|| format!("Unknown Agent: {}", assignment.name))?;
                    if !definition.enabled {
                        return Err(format!("Agent {} is disabled", definition.name));
                    }
                    if assignment.task.trim().is_empty() {
                        return Err(format!("Task for {} is empty", definition.name));
                    }
                    Ok(definition)
                })
                .collect::<Result<Vec<_>, String>>()?;
            let current_roster = app.agent_roster_for_provider();
            if !force_untracked {
                for definition in &definitions {
                    if current_roster.iter().any(|entry| {
                        entry.definition_id == definition.id
                            && entry.tracking == agent::AgentTrackingMode::Untracked
                            && (entry.busy
                                || app
                                    .agent_task_coordinator
                                    .as_ref()
                                    .is_some_and(|coordinator| coordinator.is_busy(definition.id)))
                    }) {
                        return Err(format!(
                            "Agent {} has no Hook tracking; explicitly force a second submission",
                            definition.name
                        ));
                    }
                }
            }
            for definition in &definitions {
                let _ = activate_definition(app, definition, area, zoom)?;
            }
            let roster = app.agent_roster_for_provider();
            match app.agent_task_coordinator.as_mut() {
                Some(coordinator) => coordinator
                    .sync_roster(roster)
                    .map_err(|error| error.to_string())?,
                None => {
                    app.agent_task_coordinator = Some(
                        agent::AgentTaskCoordinator::new(roster)
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            let receipts = app
                .agent_task_coordinator
                .as_mut()
                .expect("coordinator initialized above")
                .dispatch(
                    assignments
                        .into_iter()
                        .map(|assignment| {
                            agent::AgentAssignment::new(assignment.name, assignment.task)
                        })
                        .collect(),
                    force_untracked,
                )
                .map_err(|error| error.to_string())?;
            let mut wire_receipts = Vec::with_capacity(receipts.len());
            for receipt in receipts {
                let (persisted_state, persisted_tracking) = match receipt.state {
                    agent::AgentDispatchState::Queued => (
                        storage::AgentDispatchTaskState::Queued,
                        storage::AgentDispatchTracking::Managed,
                    ),
                    agent::AgentDispatchState::SubmittedManaged => (
                        storage::AgentDispatchTaskState::Submitted,
                        storage::AgentDispatchTracking::Managed,
                    ),
                    agent::AgentDispatchState::SubmittedUntracked => (
                        storage::AgentDispatchTaskState::Untracked,
                        storage::AgentDispatchTracking::Untracked,
                    ),
                };
                if let Some(store) = app.persistence.agent_dispatch_task_store() {
                    if let Err(error) = store.create(&storage::NewAgentDispatchTask {
                        definition_id: receipt.definition_id,
                        node_id: receipt.node_id,
                        original_instruction: receipt.task.clone(),
                        task_text: receipt.task.clone(),
                        state: persisted_state,
                        tracking: persisted_tracking,
                    }) {
                        tracing::error!(
                            definition_id = receipt.definition_id,
                            task_id = receipt.task_id,
                            %error,
                            "failed to persist Agent dispatch task"
                        );
                    }
                }
                let status = match receipt.state {
                    agent::AgentDispatchState::Queued => AgentTaskReceiptStatus::Queued,
                    agent::AgentDispatchState::SubmittedManaged => {
                        submit_agent_task(&mut app.runtime, receipt.node_id, &receipt.task)?;
                        AgentTaskReceiptStatus::Submitted
                    }
                    agent::AgentDispatchState::SubmittedUntracked => {
                        submit_agent_task(&mut app.runtime, receipt.node_id, &receipt.task)?;
                        AgentTaskReceiptStatus::Untracked
                    }
                };
                wire_receipts.push(AgentTaskReceipt {
                    name: receipt.name,
                    node_id: receipt.node_id,
                    task: receipt.task,
                    status,
                });
            }
            Ok(CardCommandResult::AgentTasksDispatched {
                receipts: wire_receipts,
            })
        }
        _ => Err("not a high-level Agent command".to_string()),
    }
}

pub(super) fn submit_agent_task(
    runtime: &mut runtime::Runtime,
    node_id: canvas::NodeId,
    task: &str,
) -> Result<(), String> {
    runtime
        .send_node_input(node_id, runtime::NodeInput::Text(task.to_string()))
        .map_err(|error| error.to_string())?;
    runtime
        .send_node_input(
            node_id,
            runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
            }),
        )
        .map_err(|error| error.to_string())
}

fn activate_definition(
    app: &mut MultisoulApp,
    definition: &storage::AgentDefinition,
    area: canvas::CanvasRect,
    zoom: f32,
) -> Result<(u64, bool), String> {
    if let Some(node_id) = app.runtime.document.nodes().iter().find_map(|node| {
        let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        (spec.definition_id == Some(definition.id)).then_some(node.id)
    }) {
        return Ok((node_id, true));
    }
    let bounds = area.inset(app.layout_gap_canvas());
    let (cols, rows) = crate::canvas::master_layout::term_grid_size_for_card(
        canvas::NodeType::Agent,
        bounds,
        zoom,
    );
    let node_id = app
        .runtime
        .spawn_agent_for_definition(
            definition.runtime_profile,
            definition.id,
            definition.name.clone(),
            bounds,
            cols,
            rows,
        )
        .map_err(|error| error.to_string())?;
    crate::canvas::spawn_focus::activate_spawned_node(&mut app.runtime, node_id);
    app.motion.spawn_node(node_id, bounds);
    app.motion.start_initialization(
        node_id,
        crate::motion::InitializationAnimationKind::for_agent_profile(definition.runtime_profile),
    );
    app.mark_canvas_dirty();
    app.mark_session_dirty();
    Ok((node_id, false))
}

fn canvas_agent_profile(profile: norma::protocol::AgentProfile) -> canvas::AgentProfile {
    match profile {
        norma::protocol::AgentProfile::Claude => canvas::AgentProfile::Claude,
        norma::protocol::AgentProfile::Codex => canvas::AgentProfile::Codex,
        norma::protocol::AgentProfile::Cursor => canvas::AgentProfile::Cursor,
        norma::protocol::AgentProfile::OpenCode => canvas::AgentProfile::OpenCode,
    }
}
