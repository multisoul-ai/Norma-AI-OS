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
        let AnimationWakeup::NextFrame(cadence) = self.animation_next_wakeup() else {
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
                // frame-synced 淡入收束后可能刚切到 67ms ambient timer；立即唤醒
                // timer loop 采用新 cadence，避免仍等待旧的 250ms idle 兜底。
                self.wake_animation_frame_loop();
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
        let AnimationWakeup::NextFrame(_) = self.animation_next_wakeup() else {
            return AnimationFrameCallbackOutcome::Stopped;
        };
        let animation_delta = self.animation_frame_clock.elapsed_at(now);
        let report = self.advance_animation_frame(animation_delta, cx);
        let notified = report.should_notify_surface(self.settings_panel.open);
        if notified {
            cx.notify();
        }
        let next_cadence = match self.animation_next_wakeup() {
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
        let command_drawer_active = self
            .command_dock
            .as_ref()
            .is_some_and(|state| state.drawer.is_active());
        let desktop_drawer_active = self.desktop_workspace.drawer_motion.is_active()
            && matches!(self.screen, AppScreen::DesktopWorkspace);
        let assistant_orb_visible = matches!(self.screen, AppScreen::DesktopWorkspace)
            && self.desktop_workspace.drawer_motion.left.should_render()
            && !self.motion.reduce_motion;
        let assistant_orb_engaged =
            assistant_orb_visible && self.desktop_workspace.assistant_orb_energy > 0.11;
        AnimationActivity {
            // Voice window inset owns an independent child driver. Dock and caption share
            // the root waveform schedule because both render inside workspace chrome.
            voice_active: self.voice_caption_animation_active(),
            canvas_motion_active: motion_activity.canvas_active
                || command_drawer_active
                || desktop_drawer_active
                || self.wallpaper_settings.transition_active(),
            canvas_frame_synced_active: motion_activity.frame_synced_active
                || command_drawer_active
                || desktop_drawer_active
                || assistant_orb_engaged
                || self.wallpaper_settings.transition_active(),
            canvas_ambient_active: motion_activity.ambient_active
                || self.browsers.any_loading()
                || assistant_orb_visible,
            cursor_active: self.focused_terminal_cursor_animation_active(),
            // The shared Dock also animates on the personal/workspace overview.
            // Once its transition settles, a non-Canvas screen returns to idle cadence.
            screen_is_canvas: matches!(self.screen, AppScreen::Canvas)
                || assistant_orb_visible
                || command_drawer_active
                || desktop_drawer_active
                || self.wallpaper_settings.transition_active(),
            window_active: self.window_active,
            settings_open: self.settings_panel.open,
            next_visual_boundary: motion_activity.next_visual_boundary,
        }
    }

    pub(in crate::app) fn animation_next_wakeup(&self) -> AnimationWakeup {
        AnimationSchedule::next_wakeup(
            self.animation_activity(),
            self.performance_settings
                .idle_animation_throttling_enabled(),
        )
    }

    pub(in crate::app) fn animation_timer_interval(&self, wakeup: AnimationWakeup) -> Duration {
        AnimationSchedule::timer_interval(
            wakeup,
            self.performance_settings
                .idle_animation_throttling_enabled(),
        )
    }

    pub(in crate::app) fn animation_wakeup_and_timer_interval(
        &self,
    ) -> (AnimationWakeup, Duration) {
        let wakeup = self.animation_next_wakeup();
        (wakeup, self.animation_timer_interval(wakeup))
    }

    pub(in crate::app) fn advance_animation_frame(
        &mut self,
        animation_delta: Duration,
        cx: &mut Context<Self>,
    ) -> AnimationFrameReport {
        let tick_started_at = Instant::now();
        let mut motion_changed = self.motion.tick(animation_delta);
        if let Some(state) = &mut self.command_dock {
            motion_changed |= state
                .drawer
                .tick(animation_delta, self.motion.reduce_motion);
        }
        let right_was_visible = self.desktop_workspace.drawer_motion.right.should_render();
        motion_changed |= self
            .desktop_workspace
            .drawer_motion
            .tick(animation_delta, self.motion.reduce_motion);
        if matches!(self.screen, AppScreen::DesktopWorkspace)
            && self.desktop_workspace.drawer_motion.left.should_render()
        {
            let orb_mode = self.desktop_assistant_orb_mode(cx);
            motion_changed |= self.desktop_workspace.advance_assistant_orb(
                animation_delta,
                self.motion.reduce_motion,
                orb_mode,
            );
        }
        if right_was_visible
            && !self.desktop_workspace.drawer_motion.right.should_render()
            && !self.desktop_workspace.drawers.right_open
        {
            self.hide_desktop_content_browsers();
        }
        motion_changed |= self.wallpaper_settings.tick_transition(animation_delta);
        let cursor_changed = self.advance_terminal_cursor_phase(animation_delta);
        let voice_caption_changed = self.advance_voice_caption_waveform(animation_delta);
        self.tts_preview.synchronize_external_cancellation();
        self.start_next_agent_announcement(cx);
        let diagnostics = self.motion.diagnostics();
        crate::frame_diagnostics::log_motion_sample(diagnostics);
        let report = AnimationFrameReport {
            motion_changed,
            cursor_changed,
            voice_caption_changed,
            visible_working_snake: diagnostics.chase_visible_nodes > 0,
            voice_window_inset_changed: false,
            elapsed: tick_started_at.elapsed(),
        };
        report.log_if_enabled();
        report
    }
}
