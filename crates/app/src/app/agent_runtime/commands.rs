use crate::app::MultisoulApp;
use gpui::Window;
use norma::protocol::{
    AgentSummary, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub(super) struct AgentTaskSubmitError {
    message: String,
    retry_safe: bool,
}

impl AgentTaskSubmitError {
    pub(super) fn retry_safe(&self) -> bool {
        self.retry_safe
    }
}

impl std::fmt::Display for AgentTaskSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

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
            let mut failed_tasks = HashMap::<u64, AgentTaskSubmitError>::new();
            let mut waiting_tasks = HashSet::<u64>::new();
            let mut retry_safe_managed_failures = HashSet::<i64>::new();
            let mut retry_safe_untracked_failures = HashSet::<i64>::new();
            for receipt in &receipts {
                if matches!(
                    receipt.state,
                    agent::AgentDispatchState::SubmittedManaged
                        | agent::AgentDispatchState::SubmittedUntracked
                ) {
                    if app.pending_agent_submissions.should_wait(receipt.node_id) {
                        app.pending_agent_submissions.enqueue(receipt.clone());
                        waiting_tasks.insert(receipt.task_id);
                        continue;
                    }
                    if let Err(error) =
                        submit_agent_task(&mut app.runtime, receipt.node_id, &receipt.task)
                    {
                        tracing::error!(
                            definition_id = receipt.definition_id,
                            task_id = receipt.task_id,
                            %error,
                            "failed to submit Agent task"
                        );
                        if receipt.state == agent::AgentDispatchState::SubmittedManaged
                            && error.retry_safe()
                        {
                            retry_safe_managed_failures.insert(receipt.definition_id);
                        }
                        if receipt.state == agent::AgentDispatchState::SubmittedUntracked
                            && error.retry_safe()
                        {
                            retry_safe_untracked_failures.insert(receipt.definition_id);
                        }
                        failed_tasks.insert(receipt.task_id, error);
                    }
                }
            }
            for &definition_id in &retry_safe_managed_failures {
                let task_ids = receipts
                    .iter()
                    .filter(|receipt| receipt.definition_id == definition_id)
                    .map(|receipt| receipt.task_id)
                    .collect::<Vec<_>>();
                if let Some(coordinator) = app.agent_task_coordinator.as_mut() {
                    coordinator.reject_dispatch(definition_id, &task_ids);
                }
            }
            retry_safe_untracked_failures.retain(|definition_id| {
                receipts
                    .iter()
                    .filter(|receipt| {
                        receipt.definition_id == *definition_id
                            && receipt.state == agent::AgentDispatchState::SubmittedUntracked
                    })
                    .all(|receipt| {
                        failed_tasks
                            .get(&receipt.task_id)
                            .is_some_and(AgentTaskSubmitError::retry_safe)
                    })
            });
            for &definition_id in &retry_safe_untracked_failures {
                if let Some(coordinator) = app.agent_task_coordinator.as_mut() {
                    coordinator.reject_dispatch(definition_id, &[]);
                }
            }
            let mut wire_receipts = Vec::with_capacity(receipts.len());
            for receipt in receipts {
                let submission_failed = failed_tasks.contains_key(&receipt.task_id)
                    || retry_safe_managed_failures.contains(&receipt.definition_id);
                let submission_waiting = waiting_tasks.contains(&receipt.task_id);
                let (persisted_state, persisted_tracking) = if submission_failed {
                    (
                        storage::AgentDispatchTaskState::Failed,
                        match receipt.state {
                            agent::AgentDispatchState::SubmittedUntracked => {
                                storage::AgentDispatchTracking::Untracked
                            }
                            agent::AgentDispatchState::Queued
                            | agent::AgentDispatchState::SubmittedManaged => {
                                storage::AgentDispatchTracking::Managed
                            }
                        },
                    )
                } else if submission_waiting {
                    (
                        storage::AgentDispatchTaskState::Queued,
                        match receipt.state {
                            agent::AgentDispatchState::SubmittedUntracked => {
                                storage::AgentDispatchTracking::Untracked
                            }
                            agent::AgentDispatchState::Queued
                            | agent::AgentDispatchState::SubmittedManaged => {
                                storage::AgentDispatchTracking::Managed
                            }
                        },
                    )
                } else {
                    match receipt.state {
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
                    }
                };
                if let Some(writer) = app.persistence.agent_dispatch_writer() {
                    writer.create(storage::NewAgentDispatchTask {
                        definition_id: receipt.definition_id,
                        node_id: receipt.node_id,
                        original_instruction: receipt.task.clone(),
                        task_text: receipt.task.clone(),
                        state: persisted_state,
                        tracking: persisted_tracking,
                    });
                }
                let status = if submission_failed {
                    AgentTaskReceiptStatus::Failed
                } else if submission_waiting {
                    AgentTaskReceiptStatus::Queued
                } else {
                    match receipt.state {
                        agent::AgentDispatchState::Queued => AgentTaskReceiptStatus::Queued,
                        agent::AgentDispatchState::SubmittedManaged => {
                            AgentTaskReceiptStatus::Submitted
                        }
                        agent::AgentDispatchState::SubmittedUntracked => {
                            AgentTaskReceiptStatus::Untracked
                        }
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
) -> Result<(), AgentTaskSubmitError> {
    runtime
        .send_node_input(node_id, runtime::NodeInput::Text(task.to_string()))
        .map_err(|error| AgentTaskSubmitError {
            message: error.to_string(),
            retry_safe: true,
        })?;
    if let Err(error) = runtime.send_node_input(
        node_id,
        runtime::NodeInput::Key(terminal::KeyInput {
            key: terminal::Key::Enter,
            ctrl: false,
            alt: false,
        }),
    ) {
        let cleanup = runtime.send_node_input(
            node_id,
            runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Char('u'),
                ctrl: true,
                alt: false,
            }),
        );
        return match cleanup {
            Ok(()) => Err(AgentTaskSubmitError {
                message: error.to_string(),
                retry_safe: true,
            }),
            Err(cleanup_error) => Err(AgentTaskSubmitError {
                message: format!(
                    "{error}; failed to clear unsubmitted Agent task input: {cleanup_error}"
                ),
                retry_safe: false,
            }),
        };
    }
    Ok(())
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
    app.pending_agent_submissions.mark_starting(node_id);
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
