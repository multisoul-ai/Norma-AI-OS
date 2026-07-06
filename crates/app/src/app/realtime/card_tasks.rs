use super::RealtimeControlState;
use crate::app::MultisoulApp;
use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::{execute_card_command_for_area, NormaCommandEffects};
use canvas::{CanvasNodeKind, CanvasRect};
use card_task::{
    plan_with_provider, CardCommandRunner, CardTaskEvent, CardTaskId, CardTaskPlanJsonProvider,
    CardTaskPlanner, CardTaskPlannerError, CardTaskQueue, CardTaskRequest,
};
use gpui::{Context, Window};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult};
use realtime::RealtimeToolResultStatus;
use runtime::{InputMode, Runtime};
use std::collections::HashMap;
use voice::{CardRef, VoiceCardKind, VoicePlanningRequest};

pub(crate) const DEFAULT_MAX_CARD_TASK_STEPS: usize = 8;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CardTaskRequestKey {
    intent: String,
    tool_call_id: Option<String>,
}

impl From<&CardTaskRequest> for CardTaskRequestKey {
    fn from(value: &CardTaskRequest) -> Self {
        Self {
            intent: value.intent.clone(),
            tool_call_id: value.tool_call_id.clone(),
        }
    }
}

struct UnavailableCardTaskPlannerProvider;

impl CardTaskPlanJsonProvider for UnavailableCardTaskPlannerProvider {
    fn plan_json(
        &mut self,
        _request: &VoicePlanningRequest,
    ) -> Result<String, CardTaskPlannerError> {
        Err(CardTaskPlannerError::Provider {
            message: "realtime card planner provider unavailable".to_string(),
        })
    }
}

struct StoredRequestPlanner {
    provider: Box<dyn CardTaskPlanJsonProvider>,
    requests: HashMap<CardTaskRequestKey, VoicePlanningRequest>,
}

impl StoredRequestPlanner {
    fn new(provider: Box<dyn CardTaskPlanJsonProvider>) -> Self {
        Self {
            provider,
            requests: HashMap::new(),
        }
    }

    fn insert(&mut self, request: &CardTaskRequest, planning_request: VoicePlanningRequest) {
        self.requests
            .insert(CardTaskRequestKey::from(request), planning_request);
    }

    #[cfg(test)]
    fn replace_provider(&mut self, provider: Box<dyn CardTaskPlanJsonProvider>) {
        self.provider = provider;
        self.requests.clear();
    }
}

impl CardTaskPlanner for StoredRequestPlanner {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<voice::CompiledVoicePlan, CardTaskPlannerError> {
        let key = CardTaskRequestKey::from(request);
        let planning_request =
            self.requests
                .remove(&key)
                .ok_or_else(|| CardTaskPlannerError::Provider {
                    message: format!("missing planning request snapshot for {}", request.intent),
                })?;
        plan_with_provider(self.provider.as_mut(), &planning_request)
    }
}

enum RealtimeCardTaskPlannerMode {
    PreparedJson(StoredRequestPlanner),
    #[cfg(test)]
    Direct(Box<dyn CardTaskPlanner>),
}

impl RealtimeCardTaskPlannerMode {
    fn insert_snapshot(
        &mut self,
        request: &CardTaskRequest,
        planning_request: VoicePlanningRequest,
    ) {
        match self {
            Self::PreparedJson(planner) => planner.insert(request, planning_request),
            #[cfg(test)]
            Self::Direct(_) => {}
        }
    }

    #[cfg(test)]
    fn set_compiled_planner_for_tests(&mut self, planner: Box<dyn CardTaskPlanner>) {
        *self = Self::Direct(planner);
    }

    #[cfg(test)]
    fn set_json_planner_for_tests(&mut self, planner: Box<dyn CardTaskPlanJsonProvider>) {
        match self {
            Self::PreparedJson(stored) => stored.replace_provider(planner),
            Self::Direct(_) => *self = Self::PreparedJson(StoredRequestPlanner::new(planner)),
        }
    }
}

impl Default for RealtimeCardTaskPlannerMode {
    fn default() -> Self {
        Self::PreparedJson(StoredRequestPlanner::new(Box::new(
            UnavailableCardTaskPlannerProvider,
        )))
    }
}

impl CardTaskPlanner for RealtimeCardTaskPlannerMode {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<voice::CompiledVoicePlan, CardTaskPlannerError> {
        match self {
            Self::PreparedJson(planner) => planner.plan(request),
            #[cfg(test)]
            Self::Direct(planner) => planner.plan(request),
        }
    }
}

#[derive(Default)]
pub(crate) struct RealtimeCardTaskState {
    queue: CardTaskQueue,
    planner: RealtimeCardTaskPlannerMode,
    events: Vec<CardTaskEvent>,
    tool_call_ids: HashMap<CardTaskId, Option<String>>,
    callbacks: Vec<PendingCardTaskCallback>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingCardTaskCallback {
    pub(crate) task_id: String,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) status: RealtimeToolResultStatus,
    pub(crate) summary: String,
}

impl RealtimeCardTaskState {
    pub(crate) fn enqueue_task(
        &mut self,
        intent: String,
        tool_call_id: String,
        planning_request: VoicePlanningRequest,
    ) {
        self.enqueue_task_request(
            CardTaskRequest {
                intent,
                tool_call_id: Some(tool_call_id),
            },
            planning_request,
        );
    }

    fn enqueue_task_request(
        &mut self,
        request: CardTaskRequest,
        planning_request: VoicePlanningRequest,
    ) {
        self.planner.insert_snapshot(&request, planning_request);
        let task_id = self.queue.enqueue(request.clone());
        self.tool_call_ids.insert(task_id, request.tool_call_id);
        let events = self.queue.drain_events();
        self.record_terminal_callbacks(&events);
        self.events.extend(events);
    }

    #[cfg(test)]
    pub(crate) fn enqueue_task_for_tests(
        &mut self,
        intent: String,
        tool_call_id: Option<String>,
        planning_request: VoicePlanningRequest,
    ) {
        self.enqueue_task_request(
            CardTaskRequest {
                intent,
                tool_call_id,
            },
            planning_request,
        );
    }

    pub(crate) fn cancel_task(&mut self) {
        if let Ok(event) = self.queue.cancel_latest() {
            self.record_terminal_callbacks(std::slice::from_ref(&event));
            self.events.push(event);
        }
    }

    pub(crate) fn run_next(&mut self, runner: &mut dyn CardCommandRunner) -> bool {
        let events = self.queue.run_next(&mut self.planner, runner);
        let changed = !events.is_empty();
        self.record_terminal_callbacks(&events);
        self.events.extend(events);
        changed
    }

    #[cfg(test)]
    pub(crate) fn drain_events(&mut self) -> Vec<CardTaskEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn drain_completed_callbacks(&mut self) -> Vec<PendingCardTaskCallback> {
        std::mem::take(&mut self.callbacks)
    }

    #[cfg(test)]
    pub(crate) fn set_compiled_planner_for_tests(&mut self, planner: Box<dyn CardTaskPlanner>) {
        self.planner.set_compiled_planner_for_tests(planner);
    }

    #[cfg(test)]
    pub(crate) fn set_json_planner_for_tests(
        &mut self,
        planner: Box<dyn CardTaskPlanJsonProvider>,
    ) {
        self.planner.set_json_planner_for_tests(planner);
    }

    fn record_terminal_callbacks(&mut self, events: &[CardTaskEvent]) {
        for event in events {
            let terminal = match event {
                CardTaskEvent::Succeeded { task_id, summary } => Some((
                    *task_id,
                    RealtimeToolResultStatus::Succeeded,
                    summary.clone(),
                )),
                CardTaskEvent::Failed { task_id, message } => {
                    Some((*task_id, RealtimeToolResultStatus::Failed, message.clone()))
                }
                CardTaskEvent::Cancelled { task_id, message } => Some((
                    *task_id,
                    RealtimeToolResultStatus::Cancelled,
                    message.clone(),
                )),
                _ => None,
            };

            let Some((task_id, status, summary)) = terminal else {
                continue;
            };
            let tool_call_id = self.tool_call_ids.remove(&task_id).flatten();
            self.callbacks.push(PendingCardTaskCallback {
                task_id: task_id.to_string(),
                tool_call_id,
                status,
                summary,
            });
        }
    }
}

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

impl RealtimeControlState {
    #[cfg(test)]
    pub(crate) fn drain_card_task_events(&mut self) -> Vec<CardTaskEvent> {
        self.card_tasks.drain_events()
    }

    #[cfg(test)]
    pub(crate) fn card_tasks_for_tests(&mut self) -> &mut RealtimeCardTaskState {
        &mut self.card_tasks
    }
}

impl MultisoulApp {
    pub(crate) fn enqueue_realtime_card_task(
        &mut self,
        intent: String,
        tool_call_id: String,
        cx: &mut Context<Self>,
    ) {
        let planning_request = build_realtime_card_task_planning_request(
            &self.runtime,
            intent.clone(),
            DEFAULT_MAX_CARD_TASK_STEPS,
        );
        self.realtime
            .card_tasks
            .enqueue_task(intent, tool_call_id, planning_request);
        cx.notify();
    }

    pub(crate) fn cancel_realtime_card_task(&mut self, cx: &mut Context<Self>) {
        self.realtime.card_tasks.cancel_task();
        self.flush_realtime_card_task_callbacks();
        cx.notify();
    }

    pub(crate) fn drain_realtime_card_tasks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.drain_realtime_card_tasks_for_window(window);
        if changed {
            cx.notify();
        }
        changed
    }

    fn drain_realtime_card_tasks_for_window(&mut self, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let mut runner = AppCardTaskRunner::new(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            area,
            zoom,
        );
        let changed = self.realtime.card_tasks.run_next(&mut runner);
        let effects = runner.finish();
        self.apply_card_task_effects(effects);
        self.flush_realtime_card_task_callbacks();
        changed
    }

    #[cfg(test)]
    pub(crate) fn drain_realtime_card_tasks_with_runner_for_tests(
        &mut self,
        runner: &mut dyn CardCommandRunner,
    ) -> bool {
        let changed = self.realtime.card_tasks.run_next(runner);
        self.flush_realtime_card_task_callbacks();
        changed
    }

    fn apply_card_task_effects(&mut self, effects: NormaCommandEffects) {
        if effects.canvas_dirty {
            self.mark_canvas_dirty();
        }
        if effects.session_dirty {
            self.mark_session_dirty();
        }
    }
}
