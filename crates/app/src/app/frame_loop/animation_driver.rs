use std::time::{Duration, Instant};

use gpui::{Context, Window};

use crate::app::frame_schedule::{AnimationActivity, AnimationSchedule, AnimationWakeup};
use crate::app::{AppScreen, MultisoulApp};

use super::AnimationFrameReport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum AnimationFrameCallbackOutcome {
    Deferred {
        cadence: Duration,
    },
    Stopped,
    Advanced {
        next_cadence: Option<Duration>,
        notified: bool,
    },
}

impl MultisoulApp {
    pub(crate) fn start_animation_frame_driver_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.animation_frame_pending {
            return;
        }
        let AnimationWakeup::NextFrame(cadence) =
            AnimationSchedule::next_wakeup(self.animation_activity())
        else {
            return;
        };
        self.reset_animation_frame_clock_at(cx.background_executor().now());
        self.schedule_next_animation_frame(cadence, window, cx);
    }

    fn schedule_next_animation_frame(
        &mut self,
        cadence: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.animation_frame_pending = true;
        self.animation_frame_cadence = Some(cadence);
        cx.on_next_frame(window, |app, window, cx| {
            app.on_animation_frame(window, cx);
        });
    }

    fn on_animation_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.animation_frame_pending = false;
        let Some(cadence) = self.animation_frame_cadence else {
            return;
        };
        match self.consume_animation_display_callback_at(
            cx.background_executor().now(),
            cadence,
            cx,
        ) {
            AnimationFrameCallbackOutcome::Deferred { cadence } => {
                self.schedule_next_animation_frame(cadence, window, cx);
            }
            AnimationFrameCallbackOutcome::Stopped
            | AnimationFrameCallbackOutcome::Advanced {
                next_cadence: None, ..
            } => {
                self.animation_frame_cadence = None;
            }
            AnimationFrameCallbackOutcome::Advanced {
                next_cadence: Some(cadence),
                ..
            } => self.schedule_next_animation_frame(cadence, window, cx),
        }
    }

    pub(in crate::app) fn reset_animation_frame_clock_at(&mut self, now: Instant) {
        self.animation_frame_clock.reset(now);
    }

    pub(in crate::app) fn consume_animation_display_callback_at(
        &mut self,
        now: Instant,
        cadence: Duration,
        cx: &mut Context<Self>,
    ) -> AnimationFrameCallbackOutcome {
        if !self.animation_frame_clock.is_due_at(now, cadence) {
            return AnimationFrameCallbackOutcome::Deferred { cadence };
        }
        let AnimationWakeup::NextFrame(_) =
            AnimationSchedule::next_wakeup(self.animation_activity())
        else {
            return AnimationFrameCallbackOutcome::Stopped;
        };
        let animation_delta = self.animation_frame_clock.elapsed_at(now);
        let report = self.advance_animation_frame(animation_delta, cx);
        let notified = report.should_notify_surface(self.settings_panel.open);
        if notified {
            cx.notify();
        }
        let next_cadence = match AnimationSchedule::next_wakeup(self.animation_activity()) {
            AnimationWakeup::NextFrame(cadence) => Some(cadence),
            AnimationWakeup::Timer(_) => None,
        };
        AnimationFrameCallbackOutcome::Advanced {
            next_cadence,
            notified,
        }
    }

    pub(in crate::app) fn animation_activity(&self) -> AnimationActivity {
        let motion_activity = self.motion.activity_summary();
        AnimationActivity {
            voice_active: self.voice_window_inset.is_animation_active()
                || self.has_pending_tts_playback_events(),
            canvas_motion_active: motion_activity.canvas_active,
            cursor_active: self.focused_terminal_cursor_animation_active(),
            screen_is_canvas: matches!(self.screen, AppScreen::Canvas),
            window_active: self.window_active,
            settings_open: self.settings_panel.open,
            next_visual_boundary: motion_activity.next_visual_boundary,
        }
    }

    pub(in crate::app) fn has_pending_tts_playback_events(&self) -> bool {
        self.dialogue.has_pending_playback_events()
            || self
                .agent_announcement_playback
                .has_pending_playback_events()
            || self.tts_preview.has_pending_playback_events()
    }

    pub(in crate::app) fn advance_animation_frame(
        &mut self,
        animation_delta: Duration,
        cx: &mut Context<Self>,
    ) -> AnimationFrameReport {
        let tick_started_at = Instant::now();
        let motion_changed = self.motion.tick(animation_delta);
        let cursor_changed = self.advance_terminal_cursor_phase(animation_delta);
        self.sync_voice_window_inset_enabled();
        self.tts_preview.synchronize_external_cancellation();
        for event in self.dialogue.take_playback_events() {
            self.voice_window_inset.apply_playback_event(
                crate::app::voice_window_inset::TtsPlaybackSource::Assistant,
                event,
            );
        }
        for event in self.agent_announcement_playback.take_playback_events() {
            self.voice_window_inset.apply_playback_event(
                crate::app::voice_window_inset::TtsPlaybackSource::AgentAnnouncement,
                event,
            );
        }
        for event in self.tts_preview.take_playback_events() {
            self.voice_window_inset.apply_playback_event(
                crate::app::voice_window_inset::TtsPlaybackSource::Preview,
                event,
            );
        }
        self.start_next_agent_announcement(cx);
        let voice_window_inset_changed = self.voice_window_inset.tick(animation_delta);
        let diagnostics = self.motion.diagnostics();
        crate::frame_diagnostics::log_motion_sample(diagnostics);
        let report = AnimationFrameReport {
            motion_changed,
            cursor_changed,
            visible_working_snake: diagnostics.chase_visible_nodes > 0,
            voice_window_inset_changed,
            elapsed: tick_started_at.elapsed(),
        };
        report.log_if_enabled();
        report
    }
}
