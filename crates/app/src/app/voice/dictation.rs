use crate::app::MultisoulApp;
use crate::settings::voice::VoiceInteractionMode;
use gpui::{Context, Window};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FinalVoiceTextAction {
    InsertIntoFocusedTarget,
    StartAgentTurn,
}

pub(super) fn final_voice_text_action(mode: VoiceInteractionMode) -> FinalVoiceTextAction {
    match mode {
        VoiceInteractionMode::Dictation => FinalVoiceTextAction::InsertIntoFocusedTarget,
        VoiceInteractionMode::ContinuousListening => FinalVoiceTextAction::StartAgentTurn,
    }
}

impl MultisoulApp {
    pub(super) fn handle_final_voice_text(
        &mut self,
        transcript: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match final_voice_text_action(self.voice_settings.values().interaction_mode()) {
            FinalVoiceTextAction::InsertIntoFocusedTarget => {
                if crate::app::ime::commit_text_to_focused_node(
                    &mut self.runtime,
                    self.screen,
                    &transcript,
                ) {
                    self.voice.set_executed("Dictation inserted");
                } else {
                    self.voice
                        .set_error("Focus a text-capable card before using dictation.");
                }
            }
            FinalVoiceTextAction::StartAgentTurn => {
                self.start_voice_planning(transcript, window, cx);
            }
        }
        cx.notify();
    }
}
