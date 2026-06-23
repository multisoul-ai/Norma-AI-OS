//! Keyboard routing for the root GPUI view.

use crate::app::{AppScreen, MultisoulApp};
use crate::cards::term::view::keystroke_to_input;
use crate::voice_control::{
    is_voice_hold_modifier_release, is_voice_hold_release, is_voice_hold_shortcut, VoiceTrigger,
};
use gpui::{Context, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, Window};
use runtime::{route, InputEvent, InputMode, NodeInput, Route};

impl MultisoulApp {
    /// 键盘事件：维护 space_held；聚焦终端时把按键编码写入对应 PTY。
    pub(crate) fn on_key_down(
        &mut self,
        ev: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_voice_hold_shortcut(&ev.keystroke) {
            self.begin_voice_recording(VoiceTrigger::Shortcut, cx);
            return;
        }

        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        let is_space = ev.keystroke.key == "space";
        if is_space && matches!(self.runtime.focus.mode(), InputMode::CanvasSelect) {
            self.interaction.space_held = true;
        }

        if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
            if route(&self.runtime.focus.mode(), &InputEvent::Key)
                == Route::SendInputToNode(node_id)
            {
                if ev.keystroke.modifiers.platform && ev.keystroke.key == "v" {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            if let Err(e) =
                                self.runtime.send_node_input(node_id, NodeInput::Text(text))
                            {
                                tracing::error!(error = %e, "paste send_node_input failed");
                            }
                            cx.notify();
                        }
                    }
                    return;
                }

                if crate::app_ime::should_defer_keystroke_to_ime(&ev.keystroke) {
                    return;
                }
                if let Some(input) = keystroke_to_input(&ev.keystroke) {
                    if let Err(e) = self.runtime.send_node_input(node_id, NodeInput::Key(input)) {
                        tracing::error!(error = %e, "send_node_input failed");
                    }
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn on_key_up(
        &mut self,
        ev: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_voice_recording_shortcut() && is_voice_hold_release(&ev.keystroke) {
            self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
            return;
        }

        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        if ev.keystroke.key == "space" {
            self.interaction.space_held = false;
            cx.notify();
        }
    }

    pub(crate) fn on_modifiers_changed(
        &mut self,
        ev: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_voice_recording_shortcut() && is_voice_hold_modifier_release(&ev.modifiers) {
            self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
        }
    }
}
