pub mod permissions;
pub mod session_state;

mod audio;
mod devices;
mod finalizer;
mod model;
mod resample;
mod session;

#[cfg(test)]
#[path = "../tests/unit/device_tests.rs"]
mod device_tests;

#[cfg(test)]
pub fn resolve_model_dir_for_test() -> Result<std::path::PathBuf, SpeechError> {
    model::resolve_model_dir()
}

use session::{InnerSession, SpeechFinalizer as InnerSpeechFinalizer};

pub use devices::{
    list_audio_input_devices, resolve_audio_input_device_selection, AudioInputDevice,
};

/// Check and (if not yet determined) request microphone permission.
///
/// Safe to call from a background thread — blocks until the user responds to the
/// system dialog (up to 60 s). After this returns `Ok(())`, subsequent calls to
/// `SherpaProvider::start_recording()` will not block on the permission check.
pub fn request_microphone_permission() -> Result<(), SpeechError> {
    permissions::ensure_microphone_authorized()
}

pub fn resolve_model_dir() -> Result<std::path::PathBuf, SpeechError> {
    model::resolve_model_dir()
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
    inner: InnerSession,
}

pub struct SpeechFinalizer {
    inner: InnerSpeechFinalizer,
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

    pub fn start_recording(
        &self,
    ) -> Result<(SpeechSession, async_channel::Receiver<String>), SpeechError> {
        self.start_recording_with_input_device(None)
    }

    pub fn start_recording_with_input_device(
        &self,
        input_device_id: Option<&str>,
    ) -> Result<(SpeechSession, async_channel::Receiver<String>), SpeechError> {
        session::start(
            self.model_dir.clone(),
            input_device_id.map(ToOwned::to_owned),
        )
    }
}

impl SpeechSession {
    pub fn finish(self) -> Result<String, SpeechError> {
        self.inner.finish()
    }

    pub fn begin_finish(self) -> Result<SpeechFinalizer, SpeechError> {
        self.inner
            .begin_finish()
            .map(|inner| SpeechFinalizer { inner })
    }

    pub fn cancel(self) {
        self.inner.cancel();
    }
}

impl SpeechFinalizer {
    pub fn finish(self) -> Result<String, SpeechError> {
        self.inner.finish()
    }
}
