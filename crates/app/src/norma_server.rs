use crate::app::{AppScreen, MultisoulApp};
use crate::master_layout::{
    apply_workspace_reflow_for_area, close_node_and_update_layout_for_area, layout_gap_for_zoom,
    term_grid_size_for_card,
};
use crate::motion::MotionStore;
use crate::spawn_focus::activate_spawned_node;
use canvas::{AgentProfile as CanvasAgentProfile, CanvasNodeKind, CanvasRect, NodeType};
use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::{Context, Window};
use norma::client::{decode_request_line, default_socket_path, encode_response_line};
use norma::protocol::{
    AgentProfile, CardBounds, CardCommand, CardCommandError, CardCommandResult, CardKind,
    CardSummary, CardSummaryKind, NodeId, Placement, WireRequest, WireResponse,
};
use runtime::{InputMode, NodeError, NodeInput, Runtime, DEFAULT_TERMINAL_SHELL};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

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
}

pub(crate) struct NormaServer {
    receiver: Receiver<NormaServerRequest>,
    socket_path: Option<PathBuf>,
}

struct NormaServerRequest {
    request: WireRequest,
    response: Sender<WireResponse>,
}

impl NormaServer {
    pub(crate) fn start() -> Self {
        let (sender, receiver) = unbounded();
        let socket_path = default_socket_path();
        let bound_path = bind_socket(socket_path, sender);
        Self {
            receiver,
            socket_path: bound_path,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.receiver.is_empty()
    }
}

impl Drop for NormaServer {
    fn drop(&mut self) {
        if let Some(path) = &self.socket_path {
            let _ = fs::remove_file(path);
        }
    }
}

impl MultisoulApp {
    pub(crate) fn drain_norma_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Ok(request) = self.norma_server.receiver.try_recv() {
            let response = self.execute_card_command(request.request, window);
            let _ = request.response.send(response);
            cx.notify();
        }

        if self.norma_server.has_pending() {
            cx.notify();
        }
    }

    pub(crate) fn execute_card_command(
        &mut self,
        request: WireRequest,
        window: &mut Window,
    ) -> WireResponse {
        let result = if matches!(request.command, CardCommand::List) {
            let zoom = self.runtime.document.viewport.zoom;
            execute_card_command_for_area(
                &mut self.runtime,
                &mut self.motion,
                &mut self.master_layout,
                request.command,
                CanvasRect::new(0.0, 0.0, 0.0, 0.0),
                zoom,
            )
        } else if !matches!(self.screen, AppScreen::Canvas) {
            Err(CardCommandError::AppNotReady)
        } else {
            let area = self.visible_canvas_rect(window);
            let zoom = self.runtime.document.viewport.zoom;
            execute_card_command_for_area(
                &mut self.runtime,
                &mut self.motion,
                &mut self.master_layout,
                request.command,
                area,
                zoom,
            )
        };

        let result = result.map(|execution| {
            if execution.effects.canvas_dirty {
                self.mark_canvas_dirty();
            }
            if execution.effects.session_dirty {
                self.mark_session_dirty();
            }
            execution.result
        });

        WireResponse {
            request_id: request.request_id,
            result,
        }
    }
}

pub(crate) fn execute_card_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::master_layout::MasterLayoutState,
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
        }),
    }
}

pub(crate) fn text_for_submit(mut text: String, submit: bool) -> String {
    if submit && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn bind_socket(path: PathBuf, sender: Sender<NormaServerRequest>) -> Option<PathBuf> {
    let _ = fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to bind Norma socket"
            );
            return None;
        }
    };

    let thread_path = path.clone();
    thread::Builder::new()
        .name("norma-server".to_string())
        .spawn(move || accept_loop(listener, sender))
        .map_err(|error| {
            tracing::warn!(
                path = %thread_path.display(),
                %error,
                "failed to spawn Norma server thread"
            );
        })
        .ok()?;

    Some(path)
}

fn accept_loop(listener: UnixListener, sender: Sender<NormaServerRequest>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let sender = sender.clone();
                let _ = thread::Builder::new()
                    .name("norma-client".to_string())
                    .spawn(move || handle_stream(stream, sender));
            }
            Err(error) => tracing::warn!(%error, "Norma socket accept failed"),
        }
    }
}

fn handle_stream(mut stream: UnixStream, sender: Sender<NormaServerRequest>) {
    let mut line = String::new();
    let read_result = BufReader::new(&stream).read_line(&mut line);
    let response = match read_result {
        Ok(0) => response_for_error(0, "empty Norma request"),
        Ok(_) => match decode_request_line(&line) {
            Ok(request) => dispatch_request(request, sender),
            Err(error) => response_for_error(0, &format!("invalid Norma request: {error}")),
        },
        Err(error) => response_for_error(0, &format!("could not read Norma request: {error}")),
    };

    if let Ok(line) = encode_response_line(&response) {
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
    }
}

fn dispatch_request(request: WireRequest, sender: Sender<NormaServerRequest>) -> WireResponse {
    let request_id = request.request_id;
    let (response_sender, response_receiver) = unbounded();
    if sender
        .send(NormaServerRequest {
            request,
            response: response_sender,
        })
        .is_err()
    {
        return response_for_error(request_id, "Norma app receiver is closed");
    }

    response_receiver
        .recv_timeout(RESPONSE_TIMEOUT)
        .unwrap_or_else(|_| response_for_error(request_id, "Norma request timed out"))
}

fn response_for_error(request_id: u64, message: &str) -> WireResponse {
    WireResponse {
        request_id,
        result: Err(CardCommandError::Runtime {
            message: message.to_string(),
        }),
    }
}

fn create_card(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::master_layout::MasterLayoutState,
    kind: CardKind,
    placement: Placement,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    let bounds = resolve_bounds(placement, area, zoom);
    let node_id = match kind {
        CardKind::Terminal { shell } => {
            let shell = shell.unwrap_or_else(|| DEFAULT_TERMINAL_SHELL.to_string());
            let (cols, rows) = term_grid_size_for_card(NodeType::Term, bounds, zoom);
            runtime
                .spawn_terminal(bounds, &shell, cols, rows)
                .map_err(|error| CardCommandError::Runtime {
                    message: error.to_string(),
                })?
        }
        CardKind::Agent { profile } => {
            let (cols, rows) = term_grid_size_for_card(NodeType::Agent, bounds, zoom);
            runtime
                .spawn_agent(canvas_agent_profile(profile), bounds, cols, rows)
                .map_err(|error| CardCommandError::Runtime {
                    message: error.to_string(),
                })?
        }
        CardKind::Preview { .. } => {
            return Err(CardCommandError::Unsupported {
                message: "Preview cards are not available in this baseline yet".to_string(),
            });
        }
    };

    activate_spawned_node(runtime, node_id);
    motion.spawn_node(node_id, bounds);
    apply_layout_for_current_mode(master_layout, runtime, motion, area, zoom);

    Ok(NormaExecution {
        result: CardCommandResult::Created { node_id },
        effects: NormaCommandEffects::CANVAS_SESSION,
    })
}

fn delete_card(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::master_layout::MasterLayoutState,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    if !close_node_and_update_layout_for_area(runtime, motion, master_layout, node_id, area, zoom) {
        return Err(CardCommandError::NotFound { node_id });
    }

    Ok(NormaExecution {
        result: CardCommandResult::Deleted { node_id },
        effects: NormaCommandEffects::CANVAS_SESSION,
    })
}

fn send_text(
    runtime: &mut Runtime,
    node_id: NodeId,
    text: String,
) -> Result<NormaExecution, CardCommandError> {
    runtime
        .send_node_input(node_id, NodeInput::Text(text))
        .map_err(|error| node_error(node_id, error))?;
    Ok(NormaExecution {
        result: CardCommandResult::Sent { node_id },
        effects: NormaCommandEffects::NOTIFY,
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
            kind: summary_kind(node.kind),
            selected: runtime.selection.contains(node.id),
            focused: matches!(runtime.focus.mode(), InputMode::NodeFocused(id) if id == node.id),
        })
        .collect()
}

fn summary_kind(kind: CanvasNodeKind) -> CardSummaryKind {
    match kind {
        CanvasNodeKind::Rect(_) => CardSummaryKind::Rect,
        CanvasNodeKind::Term(_) => CardSummaryKind::Terminal,
        CanvasNodeKind::Agent(spec) => CardSummaryKind::Agent {
            profile: norma_agent_profile(spec.profile),
        },
    }
}

fn apply_layout_for_current_mode(
    master_layout: &mut crate::master_layout::MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) {
    if master_layout.is_active()
        && crate::master_layout::reapply_master_layout_for_area(
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

fn canvas_agent_profile(value: AgentProfile) -> CanvasAgentProfile {
    match value {
        AgentProfile::Claude => CanvasAgentProfile::Claude,
        AgentProfile::Codex => CanvasAgentProfile::Codex,
    }
}

fn norma_agent_profile(value: CanvasAgentProfile) -> AgentProfile {
    match value {
        CanvasAgentProfile::Claude => AgentProfile::Claude,
        CanvasAgentProfile::Codex => AgentProfile::Codex,
    }
}

fn card_bounds_to_canvas(value: CardBounds) -> CanvasRect {
    CanvasRect::new(value.x, value.y, value.width, value.height)
}
