use super::{MultisoulApp, VoiceSettingsField, VoiceTextInput};
use gpui::Context;
use std::time::Duration;

const AUTO_SAVE_DELAY: Duration = Duration::from_millis(300);

impl MultisoulApp {
    pub(crate) fn begin_voice_text_input(
        &mut self,
        field: VoiceSettingsField,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(previous) = self.voice_settings.editing_field() {
            if previous != field {
                self.persist_voice_settings_in_background(cx);
                self.voice_settings_inputs.sync_field_from_state(
                    previous,
                    &self.voice_settings,
                    cx,
                );
            }
        }
        self.voice_settings.begin_edit_with_value(field, value);
        cx.notify();
    }

    pub(crate) fn update_voice_text_input(
        &mut self,
        field: VoiceSettingsField,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.update_edit_value(field, value);
        cx.notify();
    }

    pub(crate) fn auto_save_voice_text_input(
        &mut self,
        field: VoiceSettingsField,
        value: String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let saved = self.voice_settings.auto_save_edit_value(field, value);
        cx.notify();
        saved
    }

    pub(crate) fn flush_voice_text_input_autosave(&mut self, cx: &mut Context<Self>) {
        self.persist_voice_settings_in_background(cx);
    }

    pub(crate) fn commit_voice_text_input(
        &mut self,
        field: VoiceSettingsField,
        value: String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let saved = self.voice_settings.commit_edit_value(field, value);
        if saved.is_some() {
            self.persist_voice_settings_in_background(cx);
        }
        cx.notify();
        saved
    }

    pub(crate) fn cancel_voice_text_input(
        &mut self,
        field: VoiceSettingsField,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings.cancel_edit_field(field);
        cx.notify();
    }
}

impl VoiceTextInput {
    pub(super) fn auto_save_parent(&mut self, cx: &mut Context<Self>) {
        if self.marked_range.is_some() {
            return;
        }
        let value = self.content.to_string();
        if let Some(app) = self.app.upgrade() {
            let saved = app.update(cx, |app, cx| {
                app.auto_save_voice_text_input(self.field, value, cx)
            });
            if let Some(saved) = saved {
                if self.masked {
                    self.original_content = "".into();
                    self.placeholder = super::status::CONFIGURED_API_KEY_PLACEHOLDER.into();
                } else {
                    self.original_content = saved.into();
                }
                self.schedule_auto_save_flush(cx);
            }
        }
    }

    fn schedule_auto_save_flush(&mut self, cx: &mut Context<Self>) {
        let app = self.app.clone();
        self.auto_save_task = Some(cx.spawn(async move |_this, cx| {
            cx.background_executor().timer(AUTO_SAVE_DELAY).await;
            let _ = app.update(cx, |app, cx| {
                app.flush_voice_text_input_autosave(cx);
            });
        }));
    }
}
