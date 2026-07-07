pub(crate) use super::card_task_support::{
    build_realtime_card_task_planning_request, AppCardTaskRunner, DEFAULT_MAX_CARD_TASK_STEPS,
};
#[path = "card_task_planner_mode.rs"]
mod card_task_planner_mode;
#[path = "card_task_planning.rs"]
mod card_task_planning;
use super::RealtimeControlState;
use crate::app::MultisoulApp;
use crate::norma::server::NormaCommandEffects;
use card_task::{
    CardCommandRunner, CardTaskCancelError, CardTaskEvent, CardTaskId, CardTaskPlannerError,
    CardTaskQueue, CardTaskRequest,
};
#[cfg(test)]
use card_task::{CardTaskPlanJsonProvider, CardTaskPlanner};
use card_task_planner_mode::RealtimeCardTaskPlannerMode;
use card_task_planning::{run_openai_card_task_planning_job, OpenAiCardTaskPlanningJob};
use gpui::{Context, Task, Window};
use realtime::RealtimeToolResultStatus;
use std::collections::HashMap;
use voice::{CompiledVoicePlan, VoicePlanningRequest};

#[derive(Default)]
pub(crate) struct RealtimeCardTaskState {
    queue: CardTaskQueue,
    planner: RealtimeCardTaskPlannerMode,
    events: Vec<CardTaskEvent>,
    tool_call_ids: HashMap<CardTaskId, Option<String>>,
    callbacks: Vec<PendingCardTaskCallback>,
    planning_tasks: Vec<Task<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingCardTaskCallback {
    pub(crate) task_id: String,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) status: RealtimeToolResultStatus,
    pub(crate) summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CardTaskCancelOutcome {
    pub(crate) task_id: String,
    pub(crate) summary: String,
}

impl RealtimeCardTaskState {
    pub(crate) fn enqueue_task(
        &mut self,
        intent: String,
        tool_call_id: String,
        planning_request: VoicePlanningRequest,
    ) -> CardTaskRequest {
        let request = CardTaskRequest {
            intent,
            tool_call_id: Some(tool_call_id),
        };
        self.enqueue_task_request(request.clone(), planning_request);
        request
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

    pub(crate) fn cancel_task(&mut self) -> Result<CardTaskCancelOutcome, CardTaskCancelError> {
        let event = self.queue.cancel_latest()?;
        self.record_terminal_callbacks(std::slice::from_ref(&event));
        let outcome = cancel_outcome_from_event(&event);
        self.events.push(event);
        Ok(outcome)
    }

    pub(crate) fn run_next(&mut self, runner: &mut dyn CardCommandRunner) -> bool {
        let events = self.queue.run_next(&mut self.planner, runner);
        let changed = !events.is_empty();
        self.record_terminal_callbacks(&events);
        self.events.extend(events);
        changed
    }

    pub(crate) fn finish_planning(
        &mut self,
        request: CardTaskRequest,
        result: Result<CompiledVoicePlan, CardTaskPlannerError>,
    ) {
        self.planner.finish_background_result(request, result);
    }

    pub(crate) fn push_planning_task(&mut self, task: Task<()>) {
        self.planning_tasks.push(task);
    }

    #[cfg(test)]
    pub(crate) fn finish_planning_for_tests(
        &mut self,
        request: CardTaskRequest,
        result: Result<CompiledVoicePlan, CardTaskPlannerError>,
    ) {
        self.finish_planning(request, result);
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

fn cancel_outcome_from_event(event: &CardTaskEvent) -> CardTaskCancelOutcome {
    match event {
        CardTaskEvent::CancelRequested { task_id, message }
        | CardTaskEvent::Cancelled { task_id, message } => CardTaskCancelOutcome {
            task_id: task_id.to_string(),
            summary: message.clone(),
        },
        _ => unreachable!("cancel_task only returns cancellation events"),
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
        let request =
            self.realtime
                .card_tasks
                .enqueue_task(intent, tool_call_id, planning_request.clone());
        self.start_realtime_card_task_planning(request, planning_request, cx);
        cx.notify();
    }

    fn start_realtime_card_task_planning(
        &mut self,
        request: CardTaskRequest,
        planning_request: VoicePlanningRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(api_key) = self
            .voice_settings
            .values()
            .openai_api_key()
            .map(ToOwned::to_owned)
        else {
            self.realtime.card_tasks.finish_planning(
                request,
                Err(CardTaskPlannerError::Provider {
                    message: "OpenAI API key is required for card planning".to_string(),
                }),
            );
            return;
        };
        let Some(model) = self
            .voice_settings
            .values()
            .card_planner_model()
            .map(ToOwned::to_owned)
        else {
            self.realtime.card_tasks.finish_planning(
                request,
                Err(CardTaskPlannerError::Provider {
                    message: "card planner model is required".to_string(),
                }),
            );
            return;
        };

        let job = OpenAiCardTaskPlanningJob {
            request: planning_request,
            api_key,
            model,
        };
        let task = cx.spawn(async move |this, cx| {
            let planning_task = cx
                .background_executor()
                .spawn(async move { run_openai_card_task_planning_job(job) });
            let result = planning_task.await;
            let _ = this.update(cx, |app, cx| {
                app.realtime.card_tasks.finish_planning(request, result);
                cx.notify();
            });
        });
        self.realtime.card_tasks.push_planning_task(task);
    }

    #[cfg(test)]
    pub(crate) fn cancel_realtime_card_task(&mut self, cx: &mut Context<Self>) {
        let _ = self.realtime.card_tasks.cancel_task();
        self.flush_realtime_card_task_callbacks();
        cx.notify();
    }

    pub(crate) fn cancel_realtime_card_task_for_tool_call(
        &mut self,
        session_id: realtime::RealtimeSessionId,
        tool_call_id: String,
        cx: &mut Context<Self>,
    ) {
        match self.realtime.card_tasks.cancel_task() {
            Ok(outcome) => {
                self.flush_realtime_card_task_callbacks();
                self.realtime.queue_or_inject_tool_result(
                    realtime::RealtimeToolResult::task_result(
                        session_id,
                        tool_call_id,
                        RealtimeToolResultStatus::Cancelled,
                        outcome.task_id,
                        outcome.summary,
                    ),
                );
            }
            Err(error) => {
                self.realtime
                    .queue_or_inject_tool_result(realtime::RealtimeToolResult::failed(
                        session_id,
                        tool_call_id,
                        error.to_string(),
                    ));
            }
        }
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
