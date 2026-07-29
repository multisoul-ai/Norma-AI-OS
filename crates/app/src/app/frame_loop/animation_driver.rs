use std::time::{Duration, Instant};

use gpui::{Context, Window};

use crate::app::frame_schedule::{AnimationActivity, AnimationSchedule, AnimationWakeup};
use crate::app::{AppScreen, MultisoulApp};

use super::AnimationFrameReport;

impl MultisoulApp {
    pub(crate) fn start_animation_frame_driver_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.animation_frame_pending
            || !matches!(
                AnimationSchedule::next_wakeup(self.animation_activity()),
                AnimationWakeup::NextFrame(_)
            )
        {
            return;
        }
        self.animation_frame_clock
            .reset(cx.background_executor().now());
        self.schedule_next_animation_frame(window, cx);
    }

    fn schedule_next_animation_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.animation_frame_pending = true;
        cx.on_next_frame(window, |app, window, cx| {
            app.on_animation_frame(window, cx);
        });
    }

    fn on_animation_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.animation_frame_pending = false;
        let AnimationWakeup::NextFrame(cadence) =
            AnimationSchedule::next_wakeup(self.animation_activity())
        else {
            return;
        };
        let Some(animation_delta) = self
            .animation_frame_clock
            .elapsed_for_cadence_at(cx.background_executor().now(), cadence)
        else {
            self.schedule_next_animation_frame(window, cx);
            return;
        };
        let report = self.advance_animation_frame(animation_delta, cx);
        if report.should_notify_surface(self.settings_panel.open) {
            cx.notify();
        }
        if matches!(
            AnimationSchedule::next_wakeup(self.animation_activity()),
            AnimationWakeup::NextFrame(_)
        ) {
            self.schedule_next_animation_frame(window, cx);
        }
    }

    pub(super) fn animation_activity(&self) -> AnimationActivity {
        let motion_activity = self.motion.activity_summary();
        AnimationActivity {
            voice_active: self.voice_window_inset.is_animation_active(),
            canvas_motion_active: motion_activity.canvas_active,
            cursor_active: self.focused_terminal_cursor_animation_active(),
            screen_is_canvas: matches!(self.screen, AppScreen::Canvas),
            window_active: self.window_active,
            settings_open: self.settings_panel.open,
            next_visual_boundary: motion_activity.next_visual_boundary,
        }
    }

    pub(super) fn advance_animation_frame(
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
