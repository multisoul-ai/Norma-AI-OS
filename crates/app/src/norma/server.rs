pub(crate) use self::conversion::canvas_preview_type;
use self::conversion::{card_bounds_to_canvas, norma_agent_profile};
use self::orchestration::{
    agent_profile_label, is_orchestration_command, orchestration_command_label,
    orchestration_error_code,
};
use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::master_layout::{
    apply_workspace_reflow_for_area, close_node_and_update_layout_for_area, layout_gap_for_zoom,
    term_grid_size_for_card,
};
use crate::canvas::spawn_focus::activate_spawned_node;
use crate::motion::MotionStore;
use canvas::{CanvasNodeKind, CanvasRect, NodeType};
use gpui::{Context, Window};
use norma::protocol::{
    AgentErrorCode, CardCommand, CardCommandError, CardCommandResult, CardKind, CardSummary,
    CardSummaryKind, NodeId, Placement, WireRequest, WireResponse,
};
use runtime::{InputMode, NodeError, NodeInput, Runtime, DEFAULT_TERMINAL_SHELL};
use std::time::Instant;

mod conversion;
mod orchestration;
mod transport;

pub(crate) use transport::NormaServer;

#[cfg(test)]
#[path = "../../tests/unit/norma_agent_lifecycle_tests.rs"]
mod agent_lifecycle_tests;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NormaCommandEffects {
    pub(crate) canvas_dirty: bool,
    pub(crate) session_dirty: bool,
    pub(crate) notify: bool,
}

impl NormaCommandEffects {
    const NONE: Self = Self {
        canvas_dirty: false,
        session_dirty: false,
        notify: false,
    };

    const CANVAS_SESSION: Self = Self {
        canvas_dirty: true,
        session_dirty: true,
        notify: true,
    };

    const CANVAS: Self = Self {
        canvas_dirty: true,
        session_dirty: false,
        notify: true,
    };

    const NOTIFY: Self = Self {
        canvas_dirty: false,
        session_dirty: false,
        notify: true,
    };
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NormaExecution {
    pub(crate) result: CardCommandResult,
    pub(crate) effects: NormaCommandEffects,
    pub(crate) removed_agent_definition_id: Option<i64>,
}

impl MultisoulApp {
    pub(crate) fn execute_card_command(
        &mut self,
        request: WireRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> WireResponse {
        let WireRequest {
            request_id,
            authorization,
            source_node_id,
            command,
        } = request;
        let orchestration_command = is_orchestration_command(&command);
        let orchestration_started = orchestration_command.then(Instant::now);
        let orchestration_command_name =
            orchestration_command.then(|| orchestration_command_label(&command));
        let orchestration_provider = authorization
            .as_ref()
            .map(|authorization| agent_profile_label(authorization.provider));
        let result = match command {
            CardCommand::List => {
                let zoom = self.runtime.document.viewport.zoom;
                execute_card_command_for_area(
                    &mut self.runtime,
                    &mut self.motion,
                    &mut self.master_layout,
                    CardCommand::List,
                    CanvasRect::new(0.0, 0.0, 0.0, 0.0),
                    zoom,
                )
            }
            command
                if orchestration_command
                    && (!matches!(self.screen, AppScreen::Canvas)
                        || self.runtime.workspace_root().is_none()) =>
            {
                let _ = command;
                Err(CardCommandError::agent(
                    AgentErrorCode::ProjectUnavailable,
                    "当前没有打开的项目",
                    false,
                ))
            }
            command if !matches!(self.screen, AppScreen::Canvas) => {
                let _ = command;
                Err(CardCommandError::AppNotReady)
            }
            command if orchestration_command => {
                match self.validate_orchestration_source(authorization, source_node_id) {
                    Ok((source, provider)) => crate::app::execute_orchestration_agent_command(
                        self, window, command, source, provider,
                    )
                    .map(|result| NormaExecution {
                        result,
                        effects: NormaCommandEffects::NONE,
                        removed_agent_definition_id: None,
                    }),
                    Err(error) => Err(error),
                }
            }
            command @ (CardCommand::CreateAgent { .. } | CardCommand::ActivateAgent { .. }) => {
                crate::app::execute_agent_card_command(self, window, command)
                    .map(|result| NormaExecution {
                        result,
                        effects: NormaCommandEffects::NONE,
                        removed_agent_definition_id: None,
                    })
                    .map_err(|message| CardCommandError::Runtime { message })
            }
            CardCommand::Create {
                kind:
                    CardKind::Preview {
                        path,
                        explicit_type,
                    },
                placement,
            } => {
                let area = self.visible_canvas_rect(window);
                let zoom = self.runtime.document.viewport.zoom;
                let bounds = resolve_bounds(placement, area, zoom);
                let node_id = self.spawn_preview_from_path(
                    path,
                    explicit_type.map(canvas_preview_type),
                    bounds,
                    area,
                    zoom,
                    cx,
                );
                Ok(NormaExecution {
                    result: CardCommandResult::Created { node_id },
                    effects: NormaCommandEffects::NONE,
                    removed_agent_definition_id: None,
                })
            }
            command => {
                let area = self.visible_canvas_rect(window);
                let zoom = self.runtime.document.viewport.zoom;
                execute_card_command_for_area(
                    &mut self.runtime,
                    &mut self.motion,
                    &mut self.master_layout,
                    command,
                    area,
                    zoom,
                )
            }
        };

        let result = result.map(|execution| {
            if let CardCommandResult::Deleted { node_id } = &execution.result {
                self.previews.remove(*node_id);
                self.remove_browser_view(*node_id);
            }
            if execution.effects.canvas_dirty {
                self.mark_canvas_dirty();
            }
            if execution.effects.session_dirty {
                self.mark_session_dirty();
            }
            if let Some(definition_id) = execution.removed_agent_definition_id {
                if let Err(message) = self
                    .agent_definitions
                    .cleanup_quick_definition(definition_id)
                {
                    self.workspace_state.error = Some(message);
                }
            }
            execution.result
        });
        if let (Some(started), Some(command_name)) =
            (orchestration_started, orchestration_command_name)
        {
            let error_code = result
                .as_ref()
                .err()
                .and_then(orchestration_error_code)
                .unwrap_or("none");
            tracing::info!(
                target: "norma_orchestration",
                request_id,
                command = command_name,
                provider = orchestration_provider.unwrap_or("missing"),
                source_node_id = source_node_id.unwrap_or_default(),
                source_present = source_node_id.is_some(),
                ok = result.is_ok(),
                error_code,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "orchestration request completed"
            );
        }

        WireResponse { request_id, result }
    }
}

pub(crate) fn execute_card_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    command: CardCommand,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    match command {
        CardCommand::Create { kind, placement } => {
            create_card(runtime, motion, master_layout, kind, placement, area, zoom)
        }
        CardCommand::Delete { node_id } => {
            delete_card(runtime, motion, master_layout, node_id, area, zoom)
        }
        CardCommand::SendText {
            node_id,
            text,
            submit,
        } => send_text(runtime, node_id, text_for_submit(text, submit)),
        CardCommand::Focus { node_id } => focus_card(runtime, node_id),
        CardCommand::List => Ok(NormaExecution {
            result: CardCommandResult::Listed {
                cards: summarize_cards(runtime),
            },
            effects: NormaCommandEffects::NONE,
            removed_agent_definition_id: None,
        }),
        CardCommand::ListAgents
        | CardCommand::AgentTaskStatus { .. }
        | CardCommand::AgentTaskCancel { .. }
        | CardCommand::CreateAgent { .. }
        | CardCommand::ActivateAgent { .. }
        | CardCommand::DispatchAgentTasks { .. } => Err(CardCommandError::Unsupported {
            message: "high-level Agent commands require the project Agent coordinator".into(),
        }),
    }
}

pub(crate) fn text_for_submit(mut text: String, submit: bool) -> String {
    if submit && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn create_card(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    kind: CardKind,
    placement: Placement,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    let bounds = resolve_bounds(placement, area, zoom);
    match kind {
        CardKind::Terminal { shell } => {
            let shell = shell.unwrap_or_else(|| DEFAULT_TERMINAL_SHELL.to_string());
            let (cols, rows) = term_grid_size_for_card(NodeType::Term, bounds, zoom);
            let node_id = runtime
                .spawn_terminal(bounds, &shell, cols, rows)
                .map_err(|error| CardCommandError::Runtime {
                    message: error.to_string(),
                })?;
            activate_spawned_node(runtime, node_id);
            motion.spawn_node(node_id, bounds);
            motion.start_initialization(
                node_id,
                crate::motion::InitializationAnimationKind::Terminal,
            );
            apply_layout_for_current_mode(master_layout, runtime, motion, area, zoom);
            Ok(NormaExecution {
                result: CardCommandResult::Created { node_id },
                effects: NormaCommandEffects::CANVAS_SESSION,
                removed_agent_definition_id: None,
            })
        }
        CardKind::Agent { .. } => Err(CardCommandError::Unsupported {
            message: "Agent cards must be created through create_agent".into(),
        }),
        CardKind::Preview {
            path,
            explicit_type,
        } => {
            let node_id = runtime.spawn_preview_from_path(
                path,
                explicit_type.map(canvas_preview_type),
                bounds,
            );
            runtime.selection.select_only(node_id);
            runtime.focus.exit_to_canvas();
            motion.spawn_node(node_id, bounds);
            apply_layout_for_current_mode(master_layout, runtime, motion, area, zoom);
            Ok(NormaExecution {
                result: CardCommandResult::Created { node_id },
                effects: NormaCommandEffects::CANVAS,
                removed_agent_definition_id: None,
            })
        }
        CardKind::Browser { url } => {
            let node_id = runtime.spawn_browser(url, None, bounds);
            runtime.selection.select_only(node_id);
            runtime.focus.exit_to_canvas();
            motion.spawn_node(node_id, bounds);
            apply_layout_for_current_mode(master_layout, runtime, motion, area, zoom);
            Ok(NormaExecution {
                result: CardCommandResult::Created { node_id },
                effects: NormaCommandEffects::CANVAS,
                removed_agent_definition_id: None,
            })
        }
    }
}

fn delete_card(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    let removed_agent_definition_id = runtime.document.node(node_id).and_then(|node| {
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        spec.definition_id
    });
    if !close_node_and_update_layout_for_area(runtime, motion, master_layout, node_id, area, zoom) {
        return Err(CardCommandError::NotFound { node_id });
    }

    Ok(NormaExecution {
        result: CardCommandResult::Deleted { node_id },
        effects: NormaCommandEffects::CANVAS_SESSION,
        removed_agent_definition_id,
    })
}

fn send_text(
    runtime: &mut Runtime,
    node_id: NodeId,
    text: String,
) -> Result<NormaExecution, CardCommandError> {
    if runtime
        .document
        .node(node_id)
        .is_some_and(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
    {
        return Err(CardCommandError::Unsupported {
            message: "Agent text must be submitted through dispatch_agent_tasks".into(),
        });
    }
    runtime
        .send_node_input(node_id, NodeInput::Text(text))
        .map_err(|error| node_error(node_id, error))?;
    Ok(NormaExecution {
        result: CardCommandResult::Sent { node_id },
        effects: NormaCommandEffects::NOTIFY,
        removed_agent_definition_id: None,
    })
}

fn focus_card(runtime: &mut Runtime, node_id: NodeId) -> Result<NormaExecution, CardCommandError> {
    if runtime.document.node(node_id).is_none() {
        return Err(CardCommandError::NotFound { node_id });
    }
    if !runtime.node_caps(node_id).focusable {
        return Err(CardCommandError::Unsupported {
            message: format!("card {node_id} is not focusable"),
        });
    }
    runtime.selection.select_only(node_id);
    runtime.focus.focus_node(node_id);
    Ok(NormaExecution {
        result: CardCommandResult::Focused { node_id },
        effects: NormaCommandEffects::NOTIFY,
        removed_agent_definition_id: None,
    })
}

fn summarize_cards(runtime: &Runtime) -> Vec<CardSummary> {
    runtime
        .document
        .nodes()
        .iter()
        .map(|node| CardSummary {
            node_id: node.id,
            title: node.kind.display_title().to_string(),
            alias: summary_alias(&node.kind),
            kind: summary_kind(&node.kind),
            selected: runtime.selection.contains(node.id),
            focused: matches!(runtime.focus.mode(), InputMode::NodeFocused(id) if id == node.id),
        })
        .collect()
}

fn summary_alias(kind: &CanvasNodeKind) -> Option<String> {
    match kind {
        CanvasNodeKind::Agent(spec) => spec.alias.clone(),
        CanvasNodeKind::Rect(_)
        | CanvasNodeKind::Term(_)
        | CanvasNodeKind::Preview(_)
        | CanvasNodeKind::Browser(_) => None,
    }
}

fn summary_kind(kind: &CanvasNodeKind) -> CardSummaryKind {
    match kind {
        CanvasNodeKind::Rect(_) => CardSummaryKind::Rect,
        CanvasNodeKind::Term(_) => CardSummaryKind::Terminal,
        CanvasNodeKind::Agent(spec) => CardSummaryKind::Agent {
            profile: norma_agent_profile(spec.profile),
        },
        CanvasNodeKind::Preview(_) => CardSummaryKind::Preview,
        CanvasNodeKind::Browser(_) => CardSummaryKind::Browser,
    }
}

fn apply_layout_for_current_mode(
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) {
    if master_layout.is_active()
        && crate::canvas::master_layout::reapply_master_layout_for_area(
            master_layout,
            runtime,
            motion,
            area,
            zoom,
        )
    {
        return;
    }
    apply_workspace_reflow_for_area(runtime, motion, area, zoom);
}

fn resolve_bounds(placement: Placement, area: CanvasRect, zoom: f32) -> CanvasRect {
    match placement {
        Placement::VisibleArea => area.inset(layout_gap_for_zoom(zoom)),
        Placement::Bounds(bounds) => card_bounds_to_canvas(bounds),
    }
}

fn node_error(node_id: NodeId, error: NodeError) -> CardCommandError {
    match error {
        NodeError::MissingNode => CardCommandError::NotFound { node_id },
        other => CardCommandError::Runtime {
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "../../tests/unit/norma_command_tests.rs"]
mod norma_command_tests;
