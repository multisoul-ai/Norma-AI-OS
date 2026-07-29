use std::time::{Duration, Instant};

use canvas::NodeType;
use gpui::Context;
use runtime::{InputMode, NodeRunState};

use super::frame_schedule::{
    AnimationSchedule, AnimationWakeup, RepaintContext, RepaintGate, RepaintReason,
};
use super::{AppScreen, MultisoulApp};

mod animation_driver;

const RUNTIME_FRAME_INTERVAL: Duration = Duration::from_millis(16);
#[cfg(not(feature = "e2e-test-support"))]
const IDLE_ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(feature = "e2e-test-support")]
const HEADLESS_ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const PERSISTENCE_FRAME_INTERVAL: Duration = Duration::from_millis(120);

#[cfg(feature = "e2e-test-support")]
pub fn runtime_frame_interval_for_e2e() -> Duration {
    RUNTIME_FRAME_INTERVAL
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AnimationFrameClock {
    last_tick: Instant,
    accumulated: Duration,
}

impl AnimationFrameClock {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            last_tick: now,
            accumulated: Duration::ZERO,
        }
    }

    fn elapsed_at(&mut self, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        self.accumulated = Duration::ZERO;
        elapsed
    }

    fn reset(&mut self, now: Instant) {
        self.last_tick = now;
        self.accumulated = Duration::ZERO;
    }

    fn elapsed_for_cadence_at(&mut self, now: Instant, cadence: Duration) -> Option<Duration> {
        self.accumulated += now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let accumulated_nanos = self.accumulated.as_nanos();
        let cadence_nanos = cadence.as_nanos();
        let steps = accumulated_nanos / cadence_nanos;
        if steps == 0 {
            return None;
        }
        let consumed_nanos = steps * cadence_nanos;
        let remainder_nanos = accumulated_nanos - consumed_nanos;
        self.accumulated = Duration::from_nanos(remainder_nanos as u64);
        Some(Duration::from_nanos(
            consumed_nanos.min(u64::MAX as u128) as u64
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RuntimeFrameReport {
    pub runtime_changed: bool,
    pub runtime_event_count: usize,
    pub agent_hook_changed: bool,
    pub agent_hook_pending: bool,
    pub elapsed: Duration,
}

impl RuntimeFrameReport {
    pub(super) fn should_notify(self) -> bool {
        self.runtime_changed
            || self.runtime_event_count > 0
            || self.agent_hook_changed
            || self.agent_hook_pending
    }

    #[cfg(test)]
    pub(super) fn should_notify_surface(self, settings_open: bool) -> bool {
        !settings_open && self.should_notify()
    }

    pub(super) fn notify_reason_labels(self) -> String {
        let mut labels = Vec::new();
        if self.runtime_changed {
            labels.push("runtime_changed");
        }
        if self.runtime_event_count > 0 {
            labels.push("runtime_events");
        }
        if self.agent_hook_changed {
            labels.push("agent_hook_changed");
        }
        if self.agent_hook_pending {
            labels.push("agent_hook_pending");
        }
        if labels.is_empty() {
            "none".to_string()
        } else {
            labels.join("|")
        }
    }

    fn log_if_enabled(self) {
        if !crate::frame_diagnostics::enabled() {
            return;
        }
        let notify_reason_labels = self.notify_reason_labels();
        tracing::debug!(
            target: "multisoul_frame",
            elapsed_ms = self.elapsed.as_secs_f64() * 1000.0,
            should_notify = self.should_notify(),
            notify_reason_labels = notify_reason_labels.as_str(),
            runtime_changed = self.runtime_changed,
            runtime_event_count = self.runtime_event_count,
            agent_hook_changed = self.agent_hook_changed,
            agent_hook_pending = self.agent_hook_pending,
            "runtime_frame_tick"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AnimationFrameReport {
    pub motion_changed: bool,
    pub cursor_changed: bool,
    pub visible_working_snake: bool,
    pub voice_window_inset_changed: bool,
    pub elapsed: Duration,
}

impl AnimationFrameReport {
    pub(super) fn should_notify(self) -> bool {
        self.motion_changed || self.cursor_changed || self.voice_window_inset_changed
    }

    pub(super) fn should_notify_surface(self, settings_open: bool) -> bool {
        if settings_open {
            self.voice_window_inset_changed
        } else {
            self.should_notify()
        }
    }

    pub(super) fn notify_reason_labels(self) -> String {
        let mut labels = Vec::new();
        if self.motion_changed {
            labels.push("motion_changed");
        }
        if self.cursor_changed {
            labels.push("cursor_changed");
        }
        if self.visible_working_snake {
            labels.push("visible_working_snake");
        }
        if self.voice_window_inset_changed {
            labels.push("voice_window_inset_changed");
        }
        if labels.is_empty() {
            "none".to_string()
        } else {
            labels.join("|")
        }
    }

    fn log_if_enabled(self) {
        if !crate::frame_diagnostics::enabled() {
            return;
        }
        let notify_reason_labels = self.notify_reason_labels();
        tracing::debug!(
            target: "multisoul_frame",
            elapsed_ms = self.elapsed.as_secs_f64() * 1000.0,
            should_notify = self.should_notify(),
            notify_reason_labels = notify_reason_labels.as_str(),
            motion_changed = self.motion_changed,
            cursor_changed = self.cursor_changed,
            visible_working_snake = self.visible_working_snake,
            "animation_frame_tick"
        );
    }
}

fn focused_running_terminal(app: &MultisoulApp) -> bool {
    let InputMode::NodeFocused(node_id) = app.runtime.focus.mode() else {
        return false;
    };
    let Some(node) = app.runtime.document.node(node_id) else {
        return false;
    };
    matches!(node.kind.node_type(), NodeType::Term | NodeType::Agent)
        && matches!(
            app.runtime.node_run_state(node_id),
            Some(NodeRunState::Running)
        )
}

impl MultisoulApp {
    pub(super) fn start_frame_loops(cx: &mut Context<Self>) {
        Self::start_runtime_frame_loop(cx);
        Self::start_animation_frame_loop(cx);
        Self::start_persistence_frame_loop(cx);
    }

    fn start_runtime_frame_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut repaint_gate = RepaintGate::default();
            let mut repaint_diagnostics = crate::frame_diagnostics::RepaintDiagnostics::new(
                crate::frame_diagnostics::enabled(),
                cx.background_executor().now(),
            );
            loop {
                cx.background_executor().timer(RUNTIME_FRAME_INTERVAL).await;
                let frame_now = cx.background_executor().now();
                let go = this.update(cx, |app, cx| {
                    let tick_started_at = Instant::now();
                    let runtime_changed = app.runtime.drain_all();
                    let browser_changed = app.drain_browser_events();
                    let agent_hook_report =
                        app.agent_hook_server.drain_into_runtime(&mut app.runtime);
                    for event in &agent_hook_report.events {
                        app.ingest_hook_conversation_observation(event);
                        if event.provider_event == "SessionEnd" {
                            app.clear_conversation_tail_for_node(event.node_id);
                        }
                    }
                    let mut queue_changed = false;
                    for started in agent_hook_report.started_tasks {
                        queue_changed |=
                            app.register_external_agent_root(started.node_id, started.task_title);
                    }
                    for node_id in agent_hook_report.ready_node_ids {
                        queue_changed |= app.submit_pending_agent_task_for_ready_node(node_id);
                    }
                    queue_changed |= app.submit_due_pending_agent_tasks(Instant::now());
                    app.advance_session_reply_queue();
                    let speak_agent_completions = app.dialogue.is_active()
                        && app.voice_settings.values().interaction_mode()
                            == crate::settings::voice::VoiceInteractionMode::ContinuousListening;
                    for node_id in agent_hook_report.terminal_node_ids {
                        if speak_agent_completions {
                            if let Some(report) = app.agent_completion_report_for_node(node_id) {
                                app.dialogue.queue_agent_report(report);
                            }
                        }
                        queue_changed |= app.advance_agent_task_queue(node_id);
                    }
                    app.advance_session_reply_queue();
                    let runtime_events = app.runtime.take_events();
                    app.handle_agent_runtime_events(&runtime_events, cx);
                    for event in &runtime_events {
                        tracing::debug!(?event, "runtime event");
                    }
                    let report = RuntimeFrameReport {
                        runtime_changed: runtime_changed || browser_changed,
                        runtime_event_count: runtime_events.len(),
                        agent_hook_changed: agent_hook_report.changed || queue_changed,
                        agent_hook_pending: app.agent_hook_server.has_pending(),
                        elapsed: tick_started_at.elapsed(),
                    };
                    let repaint_context = RepaintContext {
                        screen_is_canvas: matches!(app.screen, AppScreen::Canvas),
                        settings_open: app.settings_panel.open,
                        window_active: app.window_active,
                        focused_running_terminal: focused_running_terminal(app),
                        voice_active: app.voice_window_inset.is_animation_active(),
                    };
                    let Some(repaint_budget) = repaint_context.repaint_budget() else {
                        repaint_gate.reset();
                        repaint_diagnostics.log_if_due(frame_now);
                        report.log_if_enabled();
                        return;
                    };
                    repaint_diagnostics.record_selected_budget(repaint_budget);
                    if report.runtime_changed {
                        let coalesced =
                            repaint_gate.request(frame_now, RepaintReason::RuntimeChanged);
                        repaint_diagnostics.record_request(coalesced);
                    }
                    if report.runtime_event_count > 0 {
                        let coalesced =
                            repaint_gate.request(frame_now, RepaintReason::RuntimeEvents);
                        repaint_diagnostics.record_request(coalesced);
                    }
                    if report.agent_hook_changed || report.agent_hook_pending {
                        let coalesced = repaint_gate.request(frame_now, RepaintReason::AgentHook);
                        repaint_diagnostics.record_request(coalesced);
                    }
                    if let Some(emission) = repaint_gate.poll(frame_now, repaint_budget) {
                        repaint_diagnostics.record_emission(emission.pending_age, repaint_budget);
                        cx.notify();
                    }
                    repaint_diagnostics.log_if_due(frame_now);
                    report.log_if_enabled();
                });
                if go.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_animation_frame_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            #[cfg(feature = "e2e-test-support")]
            let mut headless_interval = HEADLESS_ANIMATION_FRAME_INTERVAL;
            loop {
                #[cfg(feature = "e2e-test-support")]
                let timer_interval = headless_interval;
                #[cfg(not(feature = "e2e-test-support"))]
                let timer_interval = IDLE_ANIMATION_FRAME_INTERVAL;
                cx.background_executor().timer(timer_interval).await;
                let frame_now = cx.background_executor().now();
                let go = this.update(cx, |app, cx| {
                    #[cfg(not(feature = "e2e-test-support"))]
                    if app.animation_frame_pending {
                        return AnimationSchedule::next_wakeup(app.animation_activity());
                    }
                    let animation_delta = app.animation_frame_clock.elapsed_at(frame_now);
                    let report = app.advance_animation_frame(animation_delta, cx);
                    let next_wakeup = AnimationSchedule::next_wakeup(app.animation_activity());
                    #[cfg(not(feature = "e2e-test-support"))]
                    let needs_frame_driver = matches!(next_wakeup, AnimationWakeup::NextFrame(_));
                    #[cfg(feature = "e2e-test-support")]
                    let needs_frame_driver = false;
                    // Headless TestWindow 不会执行 `on_next_frame`。它已由本循环的 mock
                    // timer 推进动画，因此不能为了安装原生 frame driver 额外 notify，
                    // 否则会绕过 Runtime 的 repaint gate。
                    if report.should_notify_surface(app.settings_panel.open) || needs_frame_driver {
                        cx.notify();
                    }
                    next_wakeup
                });
                match go {
                    #[cfg(feature = "e2e-test-support")]
                    Ok(AnimationWakeup::NextFrame(interval)) => {
                        // GPUI 的 headless TestWindow 不执行 next-frame callback，
                        // 因此 L1/L2 验收继续用 mock timer 推进同一动画 seam。
                        headless_interval = interval;
                    }
                    #[cfg(feature = "e2e-test-support")]
                    Ok(AnimationWakeup::Timer(interval)) => headless_interval = interval,
                    #[cfg(not(feature = "e2e-test-support"))]
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    fn start_persistence_frame_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(PERSISTENCE_FRAME_INTERVAL)
                .await;
            let go = this.update(cx, |app, cx| {
                if app.tick_persistence(PERSISTENCE_FRAME_INTERVAL) {
                    cx.notify();
                }
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/frame_loop_tests.rs"]
mod tests;
