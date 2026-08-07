use super::{NormaCommandEffects, NormaExecution};
use crate::app::MultisoulApp;
use crate::canvas::master_layout::{
    enter_master_layout_for_area, layout_gap_for_zoom, switch_master_layout_for_area,
};
use crate::motion::MotionStore;
use canvas::{
    BrowserSurfaceKind, CanvasNodeKind, CanvasRect, EgoCardPhase, PreviewType as CanvasPreviewType,
};
use norma::presentation::validate_prepared_target;
use norma::protocol::{
    CardCommandError, CardCommandResult, EgoCaptureMode, EgoCardStatus, EgoCommand, EgoErrorCode,
    EgoOwnership as ProtocolEgoOwnership, EgoPhase, EgoPresentationDisposition,
    PresentationDisposition, PresentationErrorCode, PresentationKind, PresentationLayout,
    PresentationTarget, PreviewType,
};
use runtime::{EgoOwnership, EgoTaskCommand, EgoTaskError, Runtime};
use std::path::Path;

pub(super) fn execute_presentation_command(
    app: &mut MultisoulApp,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<MultisoulApp>,
    target: PresentationTarget,
    create_new: bool,
    source: agent::AgentDispatchSource,
) -> Result<NormaExecution, CardCommandError> {
    let source_name = source
        .definition_id
        .and_then(|definition_id| {
            app.agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == definition_id)
                .map(|definition| definition.name.clone())
        })
        .unwrap_or_else(|| "Agent".to_string());
    let area = app.visible_canvas_rect(window);
    let zoom = app.runtime.document.viewport.zoom;
    if target == PresentationTarget::Ego {
        let source_node_id = source.node_id.ok_or_else(|| {
            CardCommandError::ego(
                EgoErrorCode::Unauthorized,
                "Ego presentation requires a source Agent node",
                false,
            )
        })?;
        let previous_task_space_id = if create_new {
            app.runtime
                .ego_node_id()
                .and_then(|node_id| app.ego_task_space_id_for(node_id))
        } else {
            None
        };
        let execution = execute_ego_presentation_for_area(
            &mut app.runtime,
            &mut app.motion,
            &app.master_layout,
            create_new,
            source_node_id,
            area,
            zoom,
        )?;
        if let CardCommandResult::EgoPresented {
            status,
            disposition,
        } = &execution.result
        {
            if *disposition == EgoPresentationDisposition::Replaced {
                app.replace_ego_task_space(
                    status.node_id,
                    previous_task_space_id,
                    format!("Norma · Ego · {}", status.node_id),
                    cx,
                );
            } else {
                app.ensure_ego_task_space(status.node_id, cx);
            }
        }
        app.release_all_browser_input();
        app.focus_handle.focus(window);
        app.show_presentation_notification(source_name, cx);
        return Ok(execution);
    }
    let execution = execute_presentation_command_for_area(
        &mut app.runtime,
        &mut app.motion,
        &mut app.master_layout,
        target,
        create_new,
        area,
        zoom,
    )?;
    if let CardCommandResult::Presented {
        node_id,
        kind: PresentationKind::Preview,
        ..
    } = &execution.result
    {
        app.refresh_preview_node(*node_id, cx);
    }
    app.release_all_browser_input();
    app.focus_handle.focus(window);
    app.show_presentation_notification(source_name, cx);
    Ok(execution)
}

pub(super) fn execute_ego_presentation_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &crate::canvas::master_layout::MasterLayoutState,
    create_new: bool,
    source_node_id: u64,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    let existing = runtime.ego_node_id();
    let (node_id, disposition) = match existing {
        Some(node_id) => {
            let state = runtime
                .ego_task_state(node_id)
                .ok_or(CardCommandError::NotFound { node_id })?;
            if create_new {
                runtime
                    .replace_ego_task_space(node_id)
                    .map_err(ego_task_error)?;
                runtime
                    .apply_ego_task_command(
                        node_id,
                        EgoTaskCommand::PrepareAgent {
                            agent_node_id: source_node_id,
                        },
                    )
                    .map_err(ego_task_error)?;
                (node_id, EgoPresentationDisposition::Replaced)
            } else {
                if state.bound_agent_node_id != Some(source_node_id)
                    || !ego_phase_is_busy(state.phase)
                {
                    runtime
                        .apply_ego_task_command(
                            node_id,
                            EgoTaskCommand::PrepareAgent {
                                agent_node_id: source_node_id,
                            },
                        )
                        .map_err(ego_task_error)?;
                }
                (node_id, EgoPresentationDisposition::Reused)
            }
        }
        None => {
            let bounds = area.inset(layout_gap_for_zoom(zoom));
            let node_id = runtime.spawn_ego_browser(None, bounds);
            runtime
                .apply_ego_task_command(
                    node_id,
                    EgoTaskCommand::PrepareAgent {
                        agent_node_id: source_node_id,
                    },
                )
                .map_err(ego_task_error)?;
            motion.spawn_node(node_id, bounds);
            (node_id, EgoPresentationDisposition::Created)
        }
    };
    runtime.selection.select_only(node_id);
    runtime.focus.exit_to_canvas();
    let status = ego_status(runtime, master_layout, node_id)?;
    Ok(NormaExecution {
        result: CardCommandResult::EgoPresented {
            status,
            disposition,
        },
        effects: NormaCommandEffects::CANVAS,
        removed_agent_definition_id: None,
    })
}

pub(super) fn execute_ego_command_for_area(
    runtime: &mut Runtime,
    master_layout: &crate::canvas::master_layout::MasterLayoutState,
    node_id: u64,
    command: EgoCommand,
    source_node_id: u64,
) -> Result<NormaExecution, CardCommandError> {
    if !matches!(command, EgoCommand::Status) {
        let state = runtime
            .ego_task_state(node_id)
            .ok_or(CardCommandError::NotFound { node_id })?;
        if let Some(expected_agent) = state.bound_agent_node_id {
            if expected_agent != source_node_id {
                return Err(CardCommandError::ego(
                    EgoErrorCode::Unauthorized,
                    format!(
                        "Ego card belongs to agent {expected_agent}, not agent {source_node_id}"
                    ),
                    false,
                ));
            }
        }
    }
    let runtime_command = match command {
        EgoCommand::Status => None,
        EgoCommand::Running => Some(EgoTaskCommand::AgentRunning {
            agent_node_id: source_node_id,
        }),
        EgoCommand::Handoff { message } => Some(EgoTaskCommand::AgentHandoff {
            agent_node_id: source_node_id,
            message,
        }),
        EgoCommand::Complete => Some(EgoTaskCommand::Complete {
            agent_node_id: source_node_id,
        }),
        EgoCommand::Fail { message } => Some(EgoTaskCommand::Fail {
            agent_node_id: source_node_id,
            message,
        }),
    };
    if let Some(command) = runtime_command {
        runtime
            .apply_ego_task_command(node_id, command)
            .map_err(ego_task_error)?;
    }
    let status = ego_status(runtime, master_layout, node_id)?;
    Ok(NormaExecution {
        result: CardCommandResult::EgoStatus { status },
        effects: NormaCommandEffects::CANVAS,
        removed_agent_definition_id: None,
    })
}

pub(super) fn execute_presentation_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    target: PresentationTarget,
    create_new: bool,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    if target == PresentationTarget::Ego {
        return Err(CardCommandError::Unsupported {
            message: "Ego presentation requires an authorized source Agent".to_string(),
        });
    }
    validate_prepared_target(&target).map_err(|error| error.into_card_error())?;
    let existing = (!create_new)
        .then(|| find_matching_node(runtime, &target))
        .flatten();
    let (node_id, disposition) = match existing {
        Some(node_id) => {
            update_reused_preview_type(runtime, node_id, &target);
            (node_id, PresentationDisposition::Reused)
        }
        None => (
            create_presentation_node(runtime, motion, &target, area, zoom),
            PresentationDisposition::Created,
        ),
    };

    let arranged = if master_layout.is_active() {
        switch_master_layout_for_area(master_layout, runtime, motion, node_id, area, zoom)
    } else {
        enter_master_layout_for_area(master_layout, runtime, motion, node_id, area, zoom)
    };
    if !arranged {
        if disposition == PresentationDisposition::Created {
            runtime.remove_node(node_id);
            motion.retain_nodes(runtime.document.nodes().iter().map(|node| node.id));
        }
        return Err(CardCommandError::presentation(
            PresentationErrorCode::PresentationFailed,
            "Could not arrange the requested card as Master",
            false,
        ));
    }

    runtime.focus.exit_to_canvas();
    Ok(NormaExecution {
        result: CardCommandResult::Presented {
            node_id,
            kind: presentation_kind(&target),
            disposition,
            layout: PresentationLayout::Master,
        },
        effects: NormaCommandEffects::CANVAS,
        removed_agent_definition_id: None,
    })
}

fn find_matching_node(runtime: &Runtime, target: &PresentationTarget) -> Option<u64> {
    runtime.document.nodes().iter().find_map(|node| {
        let matches = match (&node.kind, target) {
            (CanvasNodeKind::Browser(spec), PresentationTarget::Browser { url }) => {
                browser_urls_match(&spec.url, url)
            }
            (CanvasNodeKind::Preview(spec), PresentationTarget::Preview { path, .. }) => {
                Path::new(&spec.path) == path
            }
            _ => false,
        };
        matches.then_some(node.id)
    })
}

fn browser_urls_match(current: &str, requested: &str) -> bool {
    canonical_browser_url(current)
        .zip(canonical_browser_url(requested))
        .is_some_and(|(current, requested)| current == requested)
}

fn canonical_browser_url(value: &str) -> Option<String> {
    let normalized = norma::presentation::normalize_browser_url(value).ok()?;
    url::Url::parse(&normalized).ok().map(|url| url.to_string())
}

fn create_presentation_node(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    target: &PresentationTarget,
    area: CanvasRect,
    zoom: f32,
) -> u64 {
    let bounds = area.inset(layout_gap_for_zoom(zoom));
    let node_id = match target {
        PresentationTarget::Ego => {
            unreachable!("Ego presentations use execute_ego_presentation_for_area")
        }
        PresentationTarget::Browser { url } => runtime.spawn_browser(url.clone(), None, bounds),
        PresentationTarget::Preview {
            path,
            explicit_type,
        } => runtime.spawn_preview_from_path(
            path.clone(),
            explicit_type.map(canvas_preview_type),
            bounds,
        ),
    };
    motion.spawn_node(node_id, bounds);
    node_id
}

fn update_reused_preview_type(runtime: &mut Runtime, node_id: u64, target: &PresentationTarget) {
    let PresentationTarget::Preview { explicit_type, .. } = target else {
        return;
    };
    let Some(node) = runtime.document.node_mut(node_id) else {
        return;
    };
    let CanvasNodeKind::Preview(spec) = &mut node.kind else {
        return;
    };
    spec.explicit_type = explicit_type.map(canvas_preview_type);
}

fn presentation_kind(target: &PresentationTarget) -> PresentationKind {
    match target {
        PresentationTarget::Ego => {
            unreachable!("Ego presentation has a dedicated protocol result")
        }
        PresentationTarget::Browser { .. } => PresentationKind::Browser,
        PresentationTarget::Preview { .. } => PresentationKind::Preview,
    }
}

fn ego_status(
    runtime: &Runtime,
    master_layout: &crate::canvas::master_layout::MasterLayoutState,
    node_id: u64,
) -> Result<EgoCardStatus, CardCommandError> {
    let state = runtime
        .ego_task_state(node_id)
        .ok_or(CardCommandError::NotFound { node_id })?;
    let node = runtime
        .document
        .node(node_id)
        .ok_or(CardCommandError::NotFound { node_id })?;
    let CanvasNodeKind::Browser(spec) = &node.kind else {
        return Err(CardCommandError::NotFound { node_id });
    };
    let BrowserSurfaceKind::EgoLite(spec) = &spec.surface else {
        return Err(CardCommandError::NotFound { node_id });
    };
    let replacement_pending = runtime.ego_task_space_replacement_pending(node_id);
    Ok(EgoCardStatus {
        node_id,
        task_space_id: (!replacement_pending)
            .then(|| spec.task_space_id.clone())
            .flatten(),
        task_space_name: (!replacement_pending)
            .then(|| spec.task_space_name.clone())
            .flatten(),
        phase: ego_phase(state.phase),
        ownership: ego_ownership(state.ownership),
        bound_agent_node_id: state.bound_agent_node_id,
        capture_mode: EgoCaptureMode::ExternalWindow,
        message: state.message,
        master: master_layout.master_id() == Some(node_id),
    })
}

fn ego_phase(phase: EgoCardPhase) -> EgoPhase {
    match phase {
        EgoCardPhase::Creating => EgoPhase::Creating,
        EgoCardPhase::ReadyForUser => EgoPhase::ReadyForUser,
        EgoCardPhase::PreparingAgent => EgoPhase::PreparingAgent,
        EgoCardPhase::AgentWorking => EgoPhase::AgentWorking,
        EgoCardPhase::AwaitingUser => EgoPhase::AwaitingUser,
        EgoCardPhase::UserControlling => EgoPhase::UserControlling,
        EgoCardPhase::ResumeRequested => EgoPhase::ResumeRequested,
        EgoCardPhase::Completed => EgoPhase::Completed,
        EgoCardPhase::Failed => EgoPhase::Failed,
        EgoCardPhase::Disconnected => EgoPhase::Disconnected,
    }
}

fn ego_ownership(ownership: EgoOwnership) -> ProtocolEgoOwnership {
    match ownership {
        EgoOwnership::Agent { node_id } => ProtocolEgoOwnership::Agent { node_id },
        EgoOwnership::AgentDelegatedToUser { node_id } => {
            ProtocolEgoOwnership::AgentDelegatedToUser { node_id }
        }
        EgoOwnership::User => ProtocolEgoOwnership::User,
        EgoOwnership::Inactive => ProtocolEgoOwnership::Inactive,
        EgoOwnership::Unknown => ProtocolEgoOwnership::Unknown,
    }
}

fn ego_phase_is_busy(phase: EgoCardPhase) -> bool {
    matches!(
        phase,
        EgoCardPhase::Creating
            | EgoCardPhase::PreparingAgent
            | EgoCardPhase::AgentWorking
            | EgoCardPhase::AwaitingUser
            | EgoCardPhase::UserControlling
            | EgoCardPhase::ResumeRequested
    )
}

fn ego_task_error(error: EgoTaskError) -> CardCommandError {
    match error {
        EgoTaskError::Busy { .. } => {
            CardCommandError::ego(EgoErrorCode::Busy, error.to_string(), false)
        }
        EgoTaskError::Unauthorized { .. } => {
            CardCommandError::ego(EgoErrorCode::Unauthorized, error.to_string(), false)
        }
        EgoTaskError::InvalidTransition { .. } => {
            CardCommandError::ego(EgoErrorCode::InvalidState, error.to_string(), false)
        }
        EgoTaskError::MissingNode { .. } | EgoTaskError::NotEgoNode { .. } => {
            CardCommandError::ego(EgoErrorCode::TaskSpaceMissing, error.to_string(), false)
        }
    }
}

fn canvas_preview_type(preview_type: PreviewType) -> CanvasPreviewType {
    match preview_type {
        PreviewType::Markdown => CanvasPreviewType::Markdown,
        PreviewType::Text => CanvasPreviewType::Text,
        PreviewType::Image => CanvasPreviewType::Image,
    }
}
