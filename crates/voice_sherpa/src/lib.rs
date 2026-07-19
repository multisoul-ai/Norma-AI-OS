pub mod session_state;

mod asr_stream;
mod finalizer;
mod listening_pipeline;
mod recognizer_config;
mod session;
mod tts;

#[cfg(test)]
pub fn resolve_model_dir_for_test() -> Result<std::path::PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR") {
        return Ok(path.into());
    }
    let id = asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID)
        .map_err(|error| SpeechError::ModelNotFound(error.to_string()))?;
    asr::default_model_installation_root()
        .map(|root| root.join(id.as_str()))
        .map_err(|error| SpeechError::ModelNotFound(error.to_string()))
}

use session::{InnerSession, SpeechFinalizer as InnerSpeechFinalizer};

pub use listening_pipeline::{
    apply_final_transcript_to_session, spawn_listening_pipeline_loop,
    spawn_recognizer_listening_pipeline_loop, ListeningPipelineEvent, ListeningPipelineWorker,
    SherpaListeningPipeline,
};
pub use recognizer_config::{
    build_online_recognizer_config, build_online_recognizer_config_with_hotwords, AsrHotwordConfig,
};
pub use tts::{build_zipvoice_tts_config, FakeTtsClient, SherpaTtsClient};
pub use voice_input::PcmStreamSession;
pub use voice_input::{
    list_audio_input_devices, resolve_audio_input_device_selection, AudioInputDevice,
};
pub use voice_input::{microphone_authorization_status, MicrophoneAuthorization};
pub use voice_input::{FakeEnergyVadClient, SherpaVadClient};

/// Check and (if not yet determined) request microphone permission.
///
/// Safe to call from a background thread — blocks until the user responds to the
/// system dialog (up to 60 s). After this returns `Ok(())`, subsequent calls to
/// `SherpaProvider::start_recording()` will not block on the permission check.
pub fn request_microphone_permission() -> Result<(), SpeechError> {
    voice_input::request_microphone_permission().map_err(Into::into)
}

pub fn validate_model_load(model: &asr::AvailableAsrModel) -> Result<(), SpeechError> {
    let config = build_online_recognizer_config(model)?;
    sherpa_onnx::OnlineRecognizer::create(&config)
        .map(|_| ())
        .ok_or_else(|| SpeechError::Recognizer("failed to load selected ASR model".to_string()))
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

impl From<asr::AsrError> for SpeechError {
    fn from(error: asr::AsrError) -> Self {
        Self::Recognizer(error.to_string())
    }
}

impl From<voice_input::VoiceInputError> for SpeechError {
    fn from(error: voice_input::VoiceInputError) -> Self {
        match error {
            voice_input::VoiceInputError::MicrophoneDenied => Self::MicrophoneDenied,
            voice_input::VoiceInputError::MicrophoneRestricted => Self::MicrophoneRestricted,
            voice_input::VoiceInputError::ModelNotFound(path) => Self::ModelNotFound(path),
            voice_input::VoiceInputError::Recognizer(message) => Self::Recognizer(message),
            voice_input::VoiceInputError::Audio(message) => Self::Audio(message),
        }
    }
}

pub struct SherpaProvider {
    model: asr::AvailableAsrModel,
}

pub struct SpeechSession {
    inner: InnerSession,
}

pub struct SpeechFinalizer {
    inner: InnerSpeechFinalizer,
}

impl SherpaProvider {
    pub fn new(model: asr::AvailableAsrModel) -> Self {
        Self { model }
    }

    pub fn model_dir(&self) -> &std::path::Path {
        self.model.root()
    }

    pub fn available_model(&self) -> &asr::AvailableAsrModel {
        &self.model
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
        session::start(self.model.clone(), input_device_id.map(ToOwned::to_owned))
    }

    pub fn start_pcm_stream(
        &self,
        input_device_id: Option<&str>,
    ) -> Result<
        (
            PcmStreamSession,
            async_channel::Receiver<::audio::PcmAudioChunk>,
        ),
        SpeechError,
    > {
        voice_input::start_pcm_stream(input_device_id.map(ToOwned::to_owned)).map_err(Into::into)
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
