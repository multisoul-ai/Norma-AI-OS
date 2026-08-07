//! Settings 面板文本编辑 / 快捷键录制的 KeyDown 共享逻辑。

use gpui::{Context, KeyDownEvent};

use super::MultisoulApp;

impl MultisoulApp {
    /// Settings 内文本编辑 / 快捷键录制的共享 KeyDown 处理。
    pub(crate) fn handle_settings_text_key_down(
        &mut self,
        ev: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_panel.open {
            return false;
        }
        if self.shortcuts.recording.is_some() && self.shortcuts.record_keystroke(&ev.keystroke) {
            self.persist_voice_settings_in_background(cx);
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        false
    }
}
