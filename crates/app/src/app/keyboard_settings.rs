//! Settings 面板文本编辑 / 快捷键录制的 KeyDown 共享逻辑。

use gpui::{Context, KeyDownEvent};

use super::MultisoulApp;

impl MultisoulApp {
    /// 录制、恢复默认后让系统级灵动岛热键跟随当前设置；注册失败时在设置页显示原因。
    pub(crate) fn sync_island_hotkey(&mut self, cx: &mut Context<Self>) {
        let source = self
            .shortcuts
            .current_keystroke(crate::shortcuts::ShortcutCommandId::ToggleIsland)
            .map(str::to_owned);
        if let Err(message) = crate::island::hotkey::sync(source.as_deref(), cx) {
            self.shortcuts.status = crate::shortcuts::ShortcutStatus {
                message: Some(message),
                is_error: true,
            };
        }
    }

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
            if self.shortcuts.recording.is_none() && !self.shortcuts.status.is_error {
                self.sync_desktop_shortcuts(cx);
                self.sync_island_hotkey(cx);
            }
            self.persist_voice_settings_in_background(cx);
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        false
    }
}
