use super::MultisoulApp;
use crate::app::frame_loop::AnimationFrameReport;
use gpui::Context;
use std::time::Duration;

impl MultisoulApp {
    /// 以精确逻辑时长推进 app 动效。真实窗口由 12ms 帧循环驱动；测试必须能直接验证 31ms 初始化帧和 80ms
    /// 用户中断边界，不能让帧循环量化误差改变产品时间线。
    pub fn advance_animations_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        let motion_changed = self.motion.tick(delta);
        let cursor_changed = self.advance_terminal_cursor_phase(delta);
        let voice_window_inset_changed = self.voice_window_inset.tick(delta);
        let diagnostics = self.motion.diagnostics();
        let report = AnimationFrameReport {
            motion_changed,
            cursor_changed,
            visible_working_snake: diagnostics.chase_visible_nodes > 0,
            voice_window_inset_changed,
            elapsed: delta,
        };
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
}
