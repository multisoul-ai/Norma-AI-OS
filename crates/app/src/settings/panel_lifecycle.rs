use crate::app::{AppScreen, MultisoulApp, ToggleSettings};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::VoiceRequirementState;
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

    pub(crate) fn open_voice_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        self.settings_panel.open();
        self.settings_panel.select(SettingsSection::Voice);
        self.focus_handle.focus(window);
        self.refresh_asr_model_states(cx);
        self.refresh_tts_model_states(cx);
        self.refresh_voice_microphone_requirement(cx);
        cx.notify();
    }

    pub(crate) fn refresh_voice_microphone_requirement(&mut self, cx: &mut Context<Self>) {
        let check = cx
            .background_executor()
            .spawn(async { voice_sherpa::microphone_authorization_status() });
        cx.spawn(async move |this, cx| {
            let result = check.await;
            let _ = this.update(cx, |app, cx| {
                let requirement = match result {
                    Ok(voice_sherpa::MicrophoneAuthorization::Authorized) => {
                        VoiceRequirementState::Ready
                    }
                    Ok(voice_sherpa::MicrophoneAuthorization::Denied)
                    | Ok(voice_sherpa::MicrophoneAuthorization::Restricted) => {
                        VoiceRequirementState::Failed
                    }
                    Ok(voice_sherpa::MicrophoneAuthorization::NotDetermined) => {
                        VoiceRequirementState::Missing
                    }
                    Err(_) => VoiceRequirementState::Failed,
                };
                app.voice_settings.set_microphone_requirement(requirement);
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn request_voice_microphone_access(&mut self, cx: &mut Context<Self>) {
        self.voice_settings
            .set_microphone_requirement(VoiceRequirementState::Installing {
                completed: 0,
                total: 1,
            });
        let request = cx
            .background_executor()
            .spawn(async { voice_sherpa::request_microphone_permission() });
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |app, cx| {
                app.voice_settings
                    .set_microphone_requirement(if result.is_ok() {
                        VoiceRequirementState::Ready
                    } else {
                        VoiceRequirementState::Failed
                    });
                app.complete_voice_setup_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
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
