use crate::{app::MultisoulApp, settings::voice::VoiceInteractionMode};
use gpui::{Context, Window};

impl MultisoulApp {
    pub(crate) fn activate_gpt_live_voice(
        &mut self,
        pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.values().gpt_live_interaction_mode()
            == VoiceInteractionMode::ContinuousListening
        {
            self.toggle_gpt_live_voice(window, cx);
        } else if self.gpt_live.is_active() {
            let enabled = !pointer || !self.gpt_live.input_enabled();
            if let Err(error) = self.gpt_live.request_input(enabled, !pointer) {
                self.gpt_live.fail(error);
            }
            cx.notify();
        } else {
            self.start_gpt_live_voice(true, !pointer, window, cx);
        }
    }

    pub(crate) fn release_gpt_live_input(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.gpt_live.request_input(false, false) {
            self.gpt_live.fail(error);
        }
        cx.notify();
    }

    pub(crate) fn select_gpt_live_interaction_mode(
        &mut self,
        mode: VoiceInteractionMode,
        cx: &mut Context<Self>,
    ) {
        self.stop_gpt_live_voice(cx);
        self.voice_settings.select_gpt_live_interaction_mode(mode);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }
}
