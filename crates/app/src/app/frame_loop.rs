use std::time::{Duration, Instant};

use gpui::Context;

use super::MultisoulApp;

const RUNTIME_FRAME_INTERVAL: Duration = Duration::from_millis(12);
const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(12);
const PERSISTENCE_FRAME_INTERVAL: Duration = Duration::from_millis(120);

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
    pub elapsed: Duration,
}

impl AnimationFrameReport {
    pub(super) fn should_notify(self) -> bool {
        self.motion_changed || self.cursor_changed || self.visible_working_snake
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

impl MultisoulApp {
    pub(super) fn start_frame_loops(cx: &mut Context<Self>) {
        Self::start_runtime_frame_loop(cx);
        Self::start_animation_frame_loop(cx);
        Self::start_persistence_frame_loop(cx);
    }

    fn start_runtime_frame_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(RUNTIME_FRAME_INTERVAL).await;
            let go = this.update(cx, |app, cx| {
                let tick_started_at = Instant::now();
                let runtime_changed = app.runtime.drain_all();
                let browser_changed = app.drain_browser_events();
                let agent_hook_changed = app.agent_hook_server.drain_into_runtime(&mut app.runtime);
                let runtime_events = app.runtime.take_events();
                for event in &runtime_events {
                    tracing::debug!(?event, "runtime event");
                }
                let report = RuntimeFrameReport {
                    runtime_changed: runtime_changed || browser_changed,
                    runtime_event_count: runtime_events.len(),
                    agent_hook_changed,
                    agent_hook_pending: app.agent_hook_server.has_pending(),
                    elapsed: tick_started_at.elapsed(),
                };
                if report.should_notify() {
                    cx.notify();
                }
                report.log_if_enabled();
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }

    fn start_animation_frame_loop(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut animation_interval = ANIMATION_FRAME_INTERVAL;
            loop {
                cx.background_executor().timer(animation_interval).await;
                let go = this.update(cx, |app, cx| {
                    let tick_started_at = Instant::now();
                    let motion_changed = app.motion.tick(animation_interval);
                    let cursor_changed = app.advance_terminal_cursor_phase(animation_interval);
                    let diagnostics = app.motion.diagnostics();
                    crate::frame_diagnostics::log_motion_sample(diagnostics);
                    let report = AnimationFrameReport {
                        motion_changed,
                        cursor_changed,
                        visible_working_snake: diagnostics.chase_visible_nodes > 0,
                        elapsed: tick_started_at.elapsed(),
                    };
                    if report.should_notify() {
                        cx.notify();
                    }
                    report.log_if_enabled();
                    app.motion
                        .next_animation_tick_interval(ANIMATION_FRAME_INTERVAL)
                });
                match go {
                    Ok(next_interval) => animation_interval = next_interval,
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
