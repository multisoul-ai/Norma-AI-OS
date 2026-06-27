pub mod permissions;
pub mod session_state;

mod audio;
mod model;
mod session;

#[cfg(test)]
pub use audio::audio_capture_for_test;

#[cfg(test)]
pub fn resolve_model_dir_for_test() -> Result<std::path::PathBuf, SpeechError> {
    model::resolve_model_dir()
}

use session::InnerSession;

/// Check and (if not yet determined) request microphone permission.
///
/// Safe to call from a background thread — blocks until the user responds to the
/// system dialog (up to 60 s). After this returns `Ok(())`, subsequent calls to
/// `SherpaProvider::start_recording()` will not block on the permission check.
pub fn request_microphone_permission() -> Result<(), SpeechError> {
    permissions::ensure_microphone_authorized()
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum SpeechError {
    #[error("microphone permission was denied")]
    MicrophoneDenied,
    #[error("microphone permission is restricted on this device")]
    MicrophoneRestricted,
    #[error("speech recognition produced an empty transcript")]
    EmptyTranscript,
    #[error("speech session is not recording")]
    NotRecording,
    #[error("speech session already finished")]
    AlreadyFinished,
    #[error("model not found at {0}")]
    ModelNotFound(String),
    #[error("recognizer failed: {0}")]
    Recognizer(String),
    #[error("audio engine failed: {0}")]
    Audio(String),
}

pub struct SherpaProvider {
    model_dir: std::path::PathBuf,
}

pub struct SpeechSession {
    /// Receive live partial transcription text (may arrive multiple times per second).
    pub partials: std::sync::mpsc::Receiver<String>,
    inner: InnerSession,
}

impl SherpaProvider {
    pub fn new(model_dir: &std::path::Path) -> Result<Self, SpeechError> {
        if !model_dir.exists() {
            return Err(SpeechError::ModelNotFound(model_dir.display().to_string()));
        }
        Ok(Self {
            model_dir: model_dir.to_owned(),
        })
    }

    pub fn from_env_or_default() -> Result<Self, SpeechError> {
        let dir = model::resolve_model_dir()?;
        Self::new(&dir)
    }

    pub fn start_recording(&self) -> Result<SpeechSession, SpeechError> {
        session::start(self.model_dir.clone())
    }
}

impl SpeechSession {
    pub fn finish(self) -> Result<String, SpeechError> {
        self.inner.finish()
    }

    pub fn cancel(self) {
        self.inner.cancel();
    }
}
