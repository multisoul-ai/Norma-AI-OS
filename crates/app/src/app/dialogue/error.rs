use std::fmt;
use voice_sherpa::SpeechError;

#[derive(Debug)]
pub(crate) enum DialogueStartError {
    Speech(SpeechError),
    Ollama(String),
    Provider(String),
}

impl fmt::Display for DialogueStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Speech(error) => write!(f, "{error}"),
            Self::Ollama(message) => write!(f, "Ollama unavailable: {message}"),
            Self::Provider(message) => write!(f, "Agent provider unavailable: {message}"),
        }
    }
}

impl std::error::Error for DialogueStartError {}

pub(super) fn start_error_label(error: &DialogueStartError) -> String {
    match error {
        DialogueStartError::Speech(SpeechError::MicrophoneDenied) => {
            "Microphone permission denied".to_string()
        }
        DialogueStartError::Speech(SpeechError::MicrophoneRestricted) => {
            "Microphone permission restricted".to_string()
        }
        DialogueStartError::Speech(SpeechError::ModelNotFound(path)) => {
            format!("Speech model not found: {path}")
        }
        DialogueStartError::Speech(error) => format!("Speech error: {error}"),
        DialogueStartError::Ollama(message) => format!("Ollama unavailable: {message}"),
        DialogueStartError::Provider(message) => format!("Agent provider unavailable: {message}"),
    }
}
