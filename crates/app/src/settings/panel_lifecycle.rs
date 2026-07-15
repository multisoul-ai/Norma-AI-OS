use crate::app::{AppScreen, MultisoulApp, ToggleSettings};
use gpui::{ClickEvent, Context, Window};

impl MultisoulApp {
    pub(crate) fn on_toolbar_settings(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
        if self.settings_panel.open {
            self.focus_handle.focus(window);
        }
    }

    pub(crate) fn on_toggle_settings(
        &mut self,
        _: &ToggleSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_settings_panel(cx);
        if self.settings_panel.open {
            self.focus_handle.focus(window);
        }
    }

    pub(crate) fn toggle_settings_panel(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        let was_open = self.settings_panel.open;
        self.settings_panel.toggle();
        if was_open {
            self.shortcuts.recording = None;
            self.finish_voice_settings_inputs(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_settings_panel(&mut self, cx: &mut Context<Self>) {
        self.settings_panel.close();
        self.shortcuts.recording = None;
        self.finish_voice_settings_inputs(cx);
        cx.notify();
    }

    fn finish_voice_settings_inputs(&mut self, cx: &mut Context<Self>) {
        self.flush_voice_text_input_autosave(cx);
        if let Some(field) = self.voice_settings.editing_field() {
            self.voice_settings.cancel_edit_field(field);
        }
        self.voice_settings_inputs
            .sync_from_state(&self.voice_settings, cx);
    }
}
