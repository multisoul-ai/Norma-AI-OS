use std::fmt;
use voice_sherpa::SpeechError;

#[derive(Debug)]
pub(crate) enum DialogueStartError {
    Speech(SpeechError),
    Provider(String),
}

impl fmt::Display for DialogueStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Speech(error) => write!(f, "{error}"),
            Self::Provider(message) => write!(f, "Agent provider unavailable: {message}"),
        }
    }
}

impl std::error::Error for DialogueStartError {}

pub(super) fn start_error_label(error: &DialogueStartError) -> ui::i18n::LocalizedText {
    match error {
        DialogueStartError::Speech(SpeechError::MicrophoneDenied) => {
            ui::i18n::LocalizedText::new("chrome.microphone_permission_denied")
        }
        DialogueStartError::Speech(SpeechError::MicrophoneRestricted) => {
            ui::i18n::LocalizedText::new("chrome.microphone_permission_restricted")
        }
        DialogueStartError::Speech(SpeechError::ModelNotFound(path)) => ui::i18n::message(
            "chrome.speech_model_not_found_path",
            &[("path", (path).to_string())],
        ),
        DialogueStartError::Speech(error) => ui::i18n::message(
            "chrome.speech_error_error",
            &[("error", (error).to_string())],
        ),
        DialogueStartError::Provider(message) => ui::i18n::message(
            "chrome.agent_provider_unavailable_message",
            &[("message", (message).to_string())],
        ),
    }
}
