use super::DialogueStartError;
use voice_sherpa::SpeechError;

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
