//! macOS Apple Speech microphone transcription provider.

mod permissions;
#[cfg(target_os = "macos")]
mod platform;
mod session_state;
#[cfg(not(target_os = "macos"))]
mod stub;
mod transcript;

pub use permissions::{
    microphone_authorization_from_raw, speech_authorization_from_raw, MicrophoneAuthorization,
    SpeechAuthorization,
};
pub use session_state::{SessionEvent, SessionPhase, SessionState};
pub use transcript::TranscriptAccumulator;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum SpeechError {
    #[error("speech recognition permission was denied")]
    SpeechDenied,
    #[error("speech recognition is restricted on this device")]
    SpeechRestricted,
    #[error("microphone permission was denied")]
    MicrophoneDenied,
    #[error("microphone permission is restricted on this device")]
    MicrophoneRestricted,
    #[error("speech recognizer is unavailable")]
    RecognizerUnavailable,
    #[error("speech recognition produced an empty transcript")]
    EmptyTranscript,
    #[error("speech session is not recording")]
    NotRecording,
    #[error("speech session already finished")]
    AlreadyFinished,
    #[error("speech provider failed: {0}")]
    Provider(String),
}

pub struct AppleSpeechProvider;

pub struct SpeechSession {
    inner: PlatformSpeechSession,
}

impl AppleSpeechProvider {
    pub fn default_locale() -> String {
        std::env::var("MULTISOUL_SPEECH_LOCALE").unwrap_or_else(|_| "zh-CN".to_string())
    }

    pub fn start_recording(locale: impl Into<String>) -> Result<SpeechSession, SpeechError> {
        PlatformSpeechSession::start(locale.into()).map(|inner| SpeechSession { inner })
    }
}

impl SpeechSession {
    pub fn finish(&mut self) -> Result<String, SpeechError> {
        self.inner.finish()
    }

    pub fn cancel(&mut self) {
        self.inner.cancel();
    }
}

#[cfg(target_os = "macos")]
use platform::PlatformSpeechSession;
#[cfg(not(target_os = "macos"))]
use stub::PlatformSpeechSession;
