use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::{execute_card_command_for_area, NormaCommandEffects};
use canvas::{CanvasNodeKind, CanvasRect};
use card_task::CardCommandRunner;
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult};
use runtime::{InputMode, Runtime};
use voice::{CardRef, VoiceCardKind, VoicePlanningRequest};

pub(crate) const DEFAULT_MAX_CARD_TASK_STEPS: usize = 8;

pub(crate) struct AppCardTaskRunner<'a> {
    runtime: &'a mut Runtime,
    motion: &'a mut MotionStore,
    master_layout: &'a mut MasterLayoutState,
    area: CanvasRect,
    zoom: f32,
    effects: NormaCommandEffects,
}

impl<'a> AppCardTaskRunner<'a> {
    pub(crate) fn new(
        runtime: &'a mut Runtime,
        motion: &'a mut MotionStore,
        master_layout: &'a mut MasterLayoutState,
        area: CanvasRect,
        zoom: f32,
    ) -> Self {
        Self {
            runtime,
            motion,
            master_layout,
            area,
            zoom,
            effects: NormaCommandEffects {
                canvas_dirty: false,
                session_dirty: false,
                notify: false,
            },
        }
    }

    pub(crate) fn finish(self) -> NormaCommandEffects {
        self.effects
    }
}

impl CardCommandRunner for AppCardTaskRunner<'_> {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        let execution = execute_card_command_for_area(
            self.runtime,
            self.motion,
            self.master_layout,
            command,
            self.area,
            self.zoom,
        )?;
        self.effects.canvas_dirty |= execution.effects.canvas_dirty;
        self.effects.session_dirty |= execution.effects.session_dirty;
        self.effects.notify |= execution.effects.notify;
        Ok(execution.result)
    }
}

pub(crate) fn build_realtime_card_task_planning_request(
    runtime: &Runtime,
    intent: String,
    max_steps: usize,
) -> VoicePlanningRequest {
    let selected_node_ids = runtime
        .document
        .nodes()
        .iter()
        .filter(|node| runtime.selection.contains(node.id))
        .map(|node| node.id)
        .collect();

    VoicePlanningRequest {
        transcript: intent,
        cards: runtime
            .document
            .nodes()
            .iter()
            .map(|node| CardRef {
                node_id: node.id,
                title: node.kind.display_title(),
                alias: match &node.kind {
                    CanvasNodeKind::Agent(spec) => spec.alias.clone(),
                    _ => None,
                },
                kind: voice_card_kind(&node.kind),
                selected: runtime.selection.contains(node.id),
                focused: matches!(runtime.focus.mode(), InputMode::NodeFocused(id) if id == node.id),
            })
            .collect(),
        focused_node_id: match runtime.focus.mode() {
            InputMode::NodeFocused(id) => Some(id),
            InputMode::CanvasSelect => None,
        },
        selected_node_ids,
        max_steps,
    }
}

fn voice_card_kind(kind: &CanvasNodeKind) -> VoiceCardKind {
    match kind {
        CanvasNodeKind::Term(_) => VoiceCardKind::Terminal,
        CanvasNodeKind::Agent(spec) => VoiceCardKind::Agent {
            profile: match spec.profile {
                canvas::AgentProfile::Claude => norma::protocol::AgentProfile::Claude,
                canvas::AgentProfile::Codex => norma::protocol::AgentProfile::Codex,
                canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
            },
        },
        CanvasNodeKind::Preview(_) => VoiceCardKind::Preview,
        CanvasNodeKind::Browser(_) => VoiceCardKind::Browser,
        CanvasNodeKind::Rect(_) => VoiceCardKind::Rect,
    }
}
