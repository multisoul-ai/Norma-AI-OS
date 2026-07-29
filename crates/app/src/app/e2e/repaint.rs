use super::MultisoulApp;
use crate::app::frame_loop::AnimationFrameCallbackOutcome;
use gpui::Context;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimationDisplayCallbackE2eReport {
    pub deferred: bool,
    pub state_advanced: bool,
    pub repaint_notified: bool,
    pub next_frame_requested: bool,
}

impl MultisoulApp {
    /// 以精确逻辑时长推进 app 动效。真实窗口由 12ms 帧循环驱动；测试必须能直接验证 31ms 初始化帧和 80ms
    /// 用户中断边界，不能让帧循环量化误差改变产品时间线。
    pub fn advance_animations_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        let report = self.advance_animation_frame(delta, cx);
        if report.should_notify_surface(self.settings_panel.open) {
            cx.notify();
        }
    }

    pub fn reset_canvas_render_count_for_e2e(&mut self) {
        self.canvas_render_count_for_e2e = 0;
    }

    pub fn canvas_render_count_for_e2e(&self) -> usize {
        self.canvas_render_count_for_e2e
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
}
