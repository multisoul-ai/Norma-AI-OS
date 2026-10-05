use super::*;

impl DialogueControlState {
    pub(crate) fn dialogue_voice_label(&self) -> Option<String> {
        if let Some(message) = self.error_message.as_ref() {
            return Some(message.to_string());
        }
        match self.session.phase() {
            DialoguePhase::Idle => None,
            DialoguePhase::Listening => {
                Some(ui::i18n::text("chrome.continuous_listening").to_string())
            }
            DialoguePhase::Thinking => self
                .session
                .pending_transcript()
                .filter(|transcript| !transcript.trim().is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| Some(ui::i18n::text("chrome.thinking_continuous").to_string())),
            DialoguePhase::Speaking => {
                Some(ui::i18n::text("chrome.replying_continuous").to_string())
            }
        }
    }

    pub(crate) fn effective_voice_label(&self, fallback: impl Into<String>) -> String {
        self.dialogue_voice_label()
            .unwrap_or_else(|| fallback.into())
    }
}
