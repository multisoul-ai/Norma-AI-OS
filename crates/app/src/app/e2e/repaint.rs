use super::MultisoulApp;
use crate::app::frame_loop::AnimationFrameCallbackOutcome;
use crate::app::frame_schedule::AnimationWakeup;
use canvas::NodeId;
use gpui::{App, Context};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimationDisplayCallbackE2eReport {
    pub deferred: bool,
    pub state_advanced: bool,
    pub repaint_notified: bool,
    pub next_frame_requested: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimationScheduleE2eSnapshot {
    pub next_frame_requested: bool,
    pub timer_interval: Option<Duration>,
    pub canvas_motion_active: bool,
    pub canvas_ambient_active: bool,
    pub cursor_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkingSnakeE2eSnapshot {
    pub active: bool,
    pub next_frame_requested: bool,
    pub frame_pending: bool,
    pub phase: f32,
    pub render_count: usize,
}

impl MultisoulApp {
    pub fn animation_schedule_snapshot_for_e2e(&self) -> AnimationScheduleE2eSnapshot {
        let activity = self.animation_activity();
        let wakeup = self.animation_next_wakeup();
        AnimationScheduleE2eSnapshot {
            next_frame_requested: matches!(wakeup, AnimationWakeup::NextFrame(_)),
            timer_interval: match wakeup {
                AnimationWakeup::Timer(interval) => Some(interval),
                AnimationWakeup::NextFrame(_) => None,
            },
            canvas_motion_active: activity.canvas_motion_active,
            canvas_ambient_active: activity.canvas_ambient_active,
            cursor_active: activity.cursor_active,
        }
    }

    /// 以精确逻辑时长推进 app 动效。真实窗口由 12ms 帧循环驱动；测试必须能直接验证 31ms 初始化帧和 80ms
    /// 用户中断边界，不能让帧循环量化误差改变产品时间线。
    pub fn advance_animations_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        let report = self.advance_animation_frame(delta, cx);
        if report.should_notify_surface(self.settings_panel.open) {
            cx.notify();
        }
        self.voice_window_inset_view
            .update(cx, |view, voice_cx| view.advance_for_e2e(delta, voice_cx));
    }

    pub fn reset_canvas_render_count_for_e2e(&mut self) {
        self.canvas_render_count_for_e2e = 0;
    }

    pub fn canvas_render_count_for_e2e(&self) -> usize {
        self.canvas_render_count_for_e2e
    }

    pub fn reset_working_snake_display_callback_for_e2e(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.working_snake_views.get(&node_id).cloned() else {
            return false;
        };
        let now = cx.background_executor().now();
        view.update(cx, |view, _snake_cx| {
            view.reset_display_callback_for_e2e(now)
        });
        true
    }

    pub fn reset_working_snake_render_count_for_e2e(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.working_snake_views.get(&node_id).cloned() else {
            return false;
        };
        view.update(cx, |view, _snake_cx| view.reset_render_count_for_e2e());
        true
    }

    pub fn resume_working_snake_display_driver_for_e2e(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.working_snake_views.get(&node_id).cloned() else {
            return false;
        };
        view.update(cx, |view, snake_cx| {
            view.resume_display_driver_for_e2e(snake_cx)
        });
        true
    }

    pub fn consume_working_snake_display_callback_for_e2e(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> Option<AnimationDisplayCallbackE2eReport> {
        let view = self.working_snake_views.get(&node_id)?.clone();
        let now = cx.background_executor().now();
        let outcome = view.update(cx, |view, snake_cx| {
            view.consume_display_callback_for_e2e(now, snake_cx)
        });
        Some(AnimationDisplayCallbackE2eReport {
            deferred: outcome.deferred,
            state_advanced: outcome.state_advanced,
            repaint_notified: outcome.repaint_notified,
            next_frame_requested: outcome.next_frame_requested,
        })
    }

    pub fn working_snake_snapshot_for_e2e(
        &self,
        node_id: NodeId,
        cx: &App,
    ) -> Option<WorkingSnakeE2eSnapshot> {
        let view = self.working_snake_views.get(&node_id)?;
        let now = cx.background_executor().now();
        let snapshot = view.read_with(cx, |view, _cx| view.snapshot_for_e2e(now));
        Some(WorkingSnakeE2eSnapshot {
            active: snapshot.active,
            next_frame_requested: snapshot.next_frame_requested,
            frame_pending: snapshot.frame_pending,
            phase: snapshot.phase,
            render_count: snapshot.render_count,
        })
    }

    pub fn reset_animation_display_callback_clock_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.animation_frame_loop_paused_for_e2e = true;
        self.reset_animation_frame_clock_at(cx.background_executor().now());
    }

    pub fn consume_animation_display_callback_for_e2e(
        &mut self,
        cadence: Duration,
        cx: &mut Context<Self>,
    ) -> AnimationDisplayCallbackE2eReport {
        match self.consume_animation_display_callback_at(
            cx.background_executor().now(),
            cadence,
            cx,
        ) {
            AnimationFrameCallbackOutcome::Deferred { .. } => AnimationDisplayCallbackE2eReport {
                deferred: true,
                state_advanced: false,
                repaint_notified: false,
                next_frame_requested: true,
            },
            AnimationFrameCallbackOutcome::Stopped => AnimationDisplayCallbackE2eReport {
                deferred: false,
                state_advanced: false,
                repaint_notified: false,
                next_frame_requested: false,
            },
            AnimationFrameCallbackOutcome::Advanced {
                next_cadence,
                notified,
            } => AnimationDisplayCallbackE2eReport {
                deferred: false,
                state_advanced: true,
                repaint_notified: notified,
                next_frame_requested: next_cadence.is_some(),
            },
        }
    }

    pub fn reset_voice_window_inset_display_callback_clock_for_e2e(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        self.voice_window_inset_view
            .update(cx, |view, _voice_cx| view.reset_clock_for_e2e(now));
    }

    pub fn consume_voice_window_inset_display_callback_for_e2e(
        &mut self,
        cadence: Duration,
        cx: &mut Context<Self>,
    ) -> AnimationDisplayCallbackE2eReport {
        let now = cx.background_executor().now();
        let (deferred, notified, next_frame_requested) =
            self.voice_window_inset_view.update(cx, |view, voice_cx| {
                view.consume_display_callback_for_e2e(now, cadence, voice_cx)
            });
        AnimationDisplayCallbackE2eReport {
            deferred,
            state_advanced: !deferred,
            repaint_notified: notified,
            next_frame_requested,
        }
    }
}
