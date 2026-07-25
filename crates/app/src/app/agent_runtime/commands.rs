use crate::app::MultisoulApp;
use gpui::Window;
use norma::protocol::{
    AgentErrorCode, AgentProfile, AgentSummary, AgentTaskReceipt, AgentTaskReceiptStatus,
    CardCommand, CardCommandError, CardCommandResult,
};
use std::collections::{HashMap, HashSet};

use super::command_support::{
    activate_definition, canvas_agent_profile, type_agent_task, AgentTaskSubmitError,
};
use super::pending::unsafe_submission_recovery_message;

pub(crate) fn execute_agent_card_command(
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
            | CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskCancel { .. }
    ) {
        return super::orchestration_commands::execute_high_level_agent_command(
            app,
            command,
            area,
            zoom,
            agent::AgentDispatchSource::norma(),
            None,
        )
        .map_err(|error| error.to_string());
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

pub(super) fn execute_coordinated_agent_command(
    app: &mut MultisoulApp,
    command: CardCommand,
    area: canvas::CanvasRect,
    zoom: f32,
    source: agent::AgentDispatchSource,
    source_provider: Option<AgentProfile>,
) -> Result<CardCommandResult, CardCommandError> {
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
                    tracking: tracking_label(entry.tracking).to_string(),
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
                .create_definition(name, canvas_agent_profile(profile))
                .map_err(internal_agent_error)?;
            let node_id = if activate {
                Some(
                    activate_definition(app, &definition, area, zoom)
                        .map_err(internal_agent_error)?
                        .0,
                )
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
                .ok_or_else(|| {
                    CardCommandError::agent(
                        AgentErrorCode::UnknownAgent,
                        format!("未找到 Agent：{name}"),
                        false,
                    )
                })?;
            if !definition.enabled {
                return Err(CardCommandError::agent(
                    AgentErrorCode::AgentDisabled,
                    format!("Agent {} 已禁用", definition.name),
                    false,
                ));
            }
            let (node_id, reused) =
                activate_definition(app, &definition, area, zoom).map_err(internal_agent_error)?;
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
                        .ok_or_else(|| {
                            CardCommandError::agent(
                                AgentErrorCode::UnknownAgent,
                                format!("未找到 Agent：{}", assignment.name),
                                false,
                            )
                        })?;
                    if !definition.enabled {
                        return Err(CardCommandError::agent(
                            AgentErrorCode::AgentDisabled,
                            format!("Agent {} 已禁用", definition.name),
                            false,
                        ));
                    }
                    if assignment.task.trim().is_empty() {
                        return Err(CardCommandError::agent(
                            AgentErrorCode::EmptyTask,
                            format!("给 {} 的任务不能为空", definition.name),
                            false,
                        ));
                    }
                    Ok(definition)
                })
                .collect::<Result<Vec<_>, CardCommandError>>()?;
            let current_roster = app.agent_roster_for_provider();
            let mut seen_untracked_definitions = HashSet::new();
            for definition in &definitions {
                let is_untracked = current_roster.iter().any(|entry| {
                    entry.definition_id == definition.id
                        && entry.tracking == agent::AgentTrackingMode::Untracked
                });
                if is_untracked && !seen_untracked_definitions.insert(definition.id) {
                    return Err(CardCommandError::agent(
                        AgentErrorCode::UntrackedRequiresForce,
                        format!(
                            "Agent {} 没有 Hook 跟踪；一次只能派发一个任务",
                            definition.name
                        ),
                        false,
                    ));
                }
            }
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
                        return Err(CardCommandError::agent(
                            AgentErrorCode::UntrackedRequiresForce,
                            format!(
                                "Agent {} 没有 Hook 跟踪；再次派发需要用户明确授权",
                                definition.name
                            ),
                            false,
                        ));
                    }
                }
            }
            for definition in &definitions {
                let _ = activate_definition(app, definition, area, zoom)
                    .map_err(internal_agent_error)?;
            }
            let roster = app.agent_roster_for_provider();
            match app.agent_task_coordinator.as_mut() {
                Some(coordinator) => coordinator
                    .sync_roster(roster)
                    .map_err(coordination_error)?,
                None => {
                    app.agent_task_coordinator =
                        Some(agent::AgentTaskCoordinator::new(roster).map_err(coordination_error)?);
                }
            }
            let receipts = app
                .agent_task_coordinator
                .as_mut()
                .expect("coordinator initialized above")
                .dispatch_from(
                    assignments
                        .into_iter()
                        .map(|assignment| {
                            agent::AgentAssignment::new(assignment.name, assignment.task)
                        })
                        .collect(),
                    force_untracked,
                    source,
                )
                .map_err(coordination_error)?;
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
                    match type_agent_task(&mut app.runtime, receipt.node_id, &receipt.task) {
                        Ok(()) => {
                            app.pending_agent_submissions
                                .schedule_enter(receipt.clone());
                            waiting_tasks.insert(receipt.task_id);
                        }
                        Err(error) => {
                            tracing::error!(
                                definition_id = receipt.definition_id,
                                task_id = receipt.task_id,
                                %error,
                                "failed to type Agent task"
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
            }
            if let Some(error) = failed_tasks.values().find(|error| !error.retry_safe()) {
                app.workspace_state.error =
                    Some(unsafe_submission_recovery_message(&error.message));
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
                    writer.create_orchestration(storage::NewOrchestrationAgentDispatchTask {
                        runtime_instance_id: receipt
                            .task_ref
                            .rsplit_once(':')
                            .map(|(runtime_id, _)| runtime_id.to_string())
                            .unwrap_or_default(),
                        runtime_task_id: receipt.task_id,
                        task_ref: receipt.task_ref.clone(),
                        root_task_ref: receipt.root_task_ref.clone(),
                        parent_task_ref: receipt.parent_task_ref.clone(),
                        source_definition_id: receipt.source_definition_id,
                        source_node_id: receipt.source_node_id,
                        source_provider: source_provider.map(profile_slug).map(str::to_string),
                        target_definition_id: receipt.definition_id,
                        target_node_id: receipt.node_id,
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
                    task_id: receipt.task_ref,
                    target_definition_id: receipt.definition_id,
                    name: receipt.name,
                    node_id: receipt.node_id,
                    task: receipt.task,
                    status,
                    tracking: tracking_label(match receipt.state {
                        agent::AgentDispatchState::SubmittedUntracked => {
                            agent::AgentTrackingMode::Untracked
                        }
                        agent::AgentDispatchState::Queued
                        | agent::AgentDispatchState::SubmittedManaged => {
                            agent::AgentTrackingMode::Managed
                        }
                    })
                    .to_string(),
                    parent_task_id: receipt.parent_task_ref,
                    root_task_id: receipt.root_task_ref,
                });
            }
            Ok(CardCommandResult::AgentTasksDispatched {
                receipts: wire_receipts,
            })
        }
        _ => Err(CardCommandError::Unsupported {
            message: "not a coordinated Agent command".to_string(),
        }),
    }
}

fn tracking_label(tracking: agent::AgentTrackingMode) -> &'static str {
    match tracking {
        agent::AgentTrackingMode::Managed => "managed",
        agent::AgentTrackingMode::Untracked => "untracked",
    }
}

fn profile_slug(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
    }
}

fn internal_agent_error(message: String) -> CardCommandError {
    CardCommandError::agent(AgentErrorCode::InternalError, message, false)
}

fn coordination_error(error: agent::AgentCoordinationError) -> CardCommandError {
    let code = match error {
        agent::AgentCoordinationError::UnknownName(_) => AgentErrorCode::UnknownAgent,
        agent::AgentCoordinationError::Disabled(_) => AgentErrorCode::AgentDisabled,
        agent::AgentCoordinationError::EmptyTask(_) => AgentErrorCode::EmptyTask,
        agent::AgentCoordinationError::UntrackedRequiresForce(_) => {
            AgentErrorCode::UntrackedRequiresForce
        }
        agent::AgentCoordinationError::SourceAgentUnavailable => {
            AgentErrorCode::SourceAgentUnavailable
        }
        agent::AgentCoordinationError::SelfDispatchForbidden { .. } => {
            AgentErrorCode::SelfDispatchForbidden
        }
        agent::AgentCoordinationError::DelegationCycle { .. } => AgentErrorCode::DelegationCycle,
        agent::AgentCoordinationError::DelegationBudgetExceeded { .. } => {
            AgentErrorCode::DelegationBudgetExceeded
        }
        agent::AgentCoordinationError::InvalidName(_)
        | agent::AgentCoordinationError::DuplicateName(_)
        | agent::AgentCoordinationError::Inactive(_) => AgentErrorCode::InternalError,
    };
    CardCommandError::agent(code, error.to_string(), false)
}
