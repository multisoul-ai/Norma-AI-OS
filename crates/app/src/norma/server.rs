pub(crate) use self::conversion::canvas_preview_type;
use self::conversion::node_error;
use self::layout::{apply_layout_for_new_node_or_master, resolve_bounds, summarize_cards};
use self::orchestration::{is_managed_agent_command, is_presentation_command, log_managed_request};
use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use crate::canvas::master_layout::{
    close_node_and_update_layout_for_area, term_grid_size_for_card,
};
use crate::canvas::spawn_focus::activate_spawned_node;
use crate::motion::MotionStore;
use canvas::{CanvasNodeKind, CanvasRect, NodeType};
use gpui::{Context, Window};
use norma::protocol::{
    AgentErrorCode, CardCommand, CardCommandError, CardCommandResult, CardKind, NodeId, Placement,
    WireRequest, WireResponse,
};
use runtime::{NodeInput, Runtime, DEFAULT_TERMINAL_SHELL};
use std::time::Instant;

mod conversion;
mod effects;
mod layout;
mod orchestration;
mod presentation;
mod transport;

pub(crate) use effects::{NormaCommandEffects, NormaExecution};
pub(crate) use transport::NormaServer;

#[cfg(test)]
#[path = "../../tests/unit/norma_agent_lifecycle_tests.rs"]
mod agent_lifecycle_tests;

#[cfg(test)]
#[path = "../../tests/unit/norma_presentation_lifecycle_tests.rs"]
mod presentation_lifecycle_tests;

#[cfg(test)]
#[path = "../../tests/unit/norma_ego_lifecycle_tests.rs"]
mod ego_lifecycle_tests;

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
        let managed_command = is_managed_agent_command(&command);
        let managed_started = managed_command.then(Instant::now);
        let logged_command = managed_command.then(|| command.clone());
        let logged_authorization = authorization.clone();
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
                if managed_command
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
            command if managed_command => {
                match self.validate_orchestration_source(authorization, source_node_id) {
                    Ok((source, _provider)) if is_presentation_command(&command) => {
                        let CardCommand::Present { target, create_new } = command else {
                            unreachable!("presentation classifier only accepts Present");
                        };
                        presentation::execute_presentation_command(
                            self, window, cx, target, create_new, source,
                        )
                    }
                    Ok((source, _provider)) if matches!(command, CardCommand::Ego { .. }) => {
                        let CardCommand::Ego { node_id, command } = command else {
                            unreachable!("Ego classifier only accepts Ego commands");
                        };
                        let source_node_id = source.node_id.ok_or_else(|| {
                            CardCommandError::ego(
                                norma::protocol::EgoErrorCode::Unauthorized,
                                "Ego command requires a source Agent node",
                                false,
                            )
                        });
                        source_node_id.and_then(|source_node_id| {
                            presentation::execute_ego_command_for_area(
                                &mut self.runtime,
                                &self.master_layout,
                                node_id,
                                command,
                                source_node_id,
                            )
                        })
                    }
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
                let bounds = resolve_bounds(
                    placement,
                    area,
                    zoom,
                    self.layout_strategy,
                    self.runtime.document.nodes().len(),
                );
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
                execute_card_command_for_area_with_strategy(
                    &mut self.runtime,
                    &mut self.motion,
                    &mut self.master_layout,
                    self.layout_strategy,
                    command,
                    area,
                    zoom,
                )
            }
        };

        let result = result.map(|execution| {
            if let CardCommandResult::Deleted { node_id } = &execution.result {
                self.terminal_render_cache
                    .borrow_mut()
                    .remove_node(*node_id);
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
        if let (Some(started), Some(command)) = (managed_started, logged_command.as_ref()) {
            log_managed_request(
                request_id,
                command,
                logged_authorization.as_ref(),
                source_node_id,
                &result,
                started.elapsed(),
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
    execute_card_command_for_area_with_strategy(
        runtime,
        motion,
        master_layout,
        CanvasLayoutStrategy::default(),
        command,
        area,
        zoom,
    )
}

pub(crate) fn execute_card_command_for_area_with_strategy(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    strategy: CanvasLayoutStrategy,
    command: CardCommand,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    match command {
        CardCommand::Create { kind, placement } => create_card(
            runtime,
            motion,
            master_layout,
            strategy,
            kind,
            placement,
            area,
            zoom,
        ),
        CardCommand::Delete { node_id } => delete_card(
            runtime,
            motion,
            master_layout,
            strategy,
            node_id,
            area,
            zoom,
        ),
        CardCommand::Present { target, create_new } => {
            presentation::execute_presentation_command_for_area(
                runtime,
                motion,
                master_layout,
                target,
                create_new,
                area,
                zoom,
            )
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
        | CardCommand::DispatchAgentTasks { .. }
        | CardCommand::Ego { .. } => Err(CardCommandError::Unsupported {
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
    strategy: CanvasLayoutStrategy,
    kind: CardKind,
    placement: Placement,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    let bounds = resolve_bounds(
        placement,
        area,
        zoom,
        strategy,
        runtime.document.nodes().len(),
    );
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
            apply_layout_for_new_node_or_master(
                master_layout,
                runtime,
                motion,
                node_id,
                area,
                zoom,
                strategy,
            );
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
            apply_layout_for_new_node_or_master(
                master_layout,
                runtime,
                motion,
                node_id,
                area,
                zoom,
                strategy,
            );
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
            apply_layout_for_new_node_or_master(
                master_layout,
                runtime,
                motion,
                node_id,
                area,
                zoom,
                strategy,
            );
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
    strategy: CanvasLayoutStrategy,
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
    if !close_node_and_update_layout_for_area(
        runtime,
        motion,
        master_layout,
        node_id,
        area,
        zoom,
        strategy,
    ) {
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

#[cfg(test)]
#[path = "../../tests/unit/norma_command_tests.rs"]
mod norma_command_tests;

#[cfg(test)]
#[path = "../../tests/unit/norma_presentation_command_tests.rs"]
mod norma_presentation_command_tests;
