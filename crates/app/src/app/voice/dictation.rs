use crate::app::MultisoulApp;
use crate::settings::voice::VoiceInteractionMode;
use gpui::{Context, Window};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FinalVoiceTextAction {
    StartAgentTurn,
}

pub(super) fn final_voice_text_action(mode: VoiceInteractionMode) -> FinalVoiceTextAction {
    let _ = mode;
    FinalVoiceTextAction::StartAgentTurn
}

impl MultisoulApp {
    pub(super) fn handle_final_voice_text(
        &mut self,
        transcript: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match final_voice_text_action(self.voice_settings.values().interaction_mode()) {
            FinalVoiceTextAction::StartAgentTurn => {
                match self.dialogue.begin_single_voice_command(&transcript) {
                    Ok(()) => {
                        self.record_assistant_user_message(&transcript, cx);
                        self.voice
                            .set_executed(ui::i18n::text("chrome.voice_command_sent"));
                    }
                    Err(error) => self.voice.set_speech_error(&error),
                }
            }
        }
        cx.notify();
    }
}
