mod audio_capture;
mod devices;
mod permissions;
mod resample;
mod samples;
mod stream;
mod vad;

pub use audio_capture::{
    pcm16_bytes_from_f32_samples, pcm_stream_chunk_from_samples, AudioCapture,
    PCM_STREAM_SAMPLE_RATE_HZ, RECOGNIZER_SAMPLE_RATE_HZ,
};
pub use devices::{
    list_audio_input_devices, resolve_audio_input_device_selection, AudioInputDevice,
};
pub use permissions::{
    ensure_microphone_authorized, microphone_authorization_status, MicrophoneAuthorization,
};
#[cfg(feature = "test-support")]
pub use stream::install_test_pcm_stream;
pub use stream::{start_pcm_stream, PcmStreamSession};
pub use vad::{FakeEnergyVadClient, SherpaVadClient};

use std::sync::atomic::{AtomicUsize, Ordering};

pub const PCM_QUEUE_CHUNKS: usize = 30;

#[derive(Debug)]
pub enum PcmEvent {
    Samples(Vec<f32>),
    EndOfAudio,
}

#[derive(Debug, Default)]
pub struct AudioQueueStats {
    sent_chunks: AtomicUsize,
    dropped_chunks: AtomicUsize,
}

impl AudioQueueStats {
    pub fn record_sent_chunk(&self) {
        self.sent_chunks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_dropped_chunk(&self) {
        self.dropped_chunks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sent_chunks(&self) -> usize {
        self.sent_chunks.load(Ordering::Relaxed)
    }

    pub fn dropped_chunks(&self) -> usize {
        self.dropped_chunks.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum VoiceInputError {
    #[error("microphone permission was denied")]
    MicrophoneDenied,
    #[error("microphone permission is restricted on this device")]
    MicrophoneRestricted,
    #[error("model not found at {0}")]
    ModelNotFound(String),
    #[error("voice activity detector failed: {0}")]
    Recognizer(String),
    #[error("audio engine failed: {0}")]
    Audio(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}

pub trait VadClient: Send {
    fn push_pcm(&mut self, chunk: &audio::PcmAudioChunk) -> Result<Vec<VadEvent>, VoiceInputError>;
    fn reset(&mut self);
}

pub fn request_microphone_permission() -> Result<(), VoiceInputError> {
    #[cfg(feature = "test-support")]
    {
        return Ok(());
    }
    #[cfg(not(feature = "test-support"))]
    {
        ensure_microphone_authorized()
    }
}

#[cfg(test)]
pub fn resolve_model_dir_for_test() -> Result<std::path::PathBuf, VoiceInputError> {
    if let Ok(path) = std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR") {
        return Ok(path.into());
    }
    let id = asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID)
        .map_err(|error| VoiceInputError::ModelNotFound(error.to_string()))?;
    asr::default_model_installation_root()
        .map(|root| root.join(id.as_str()))
        .map_err(|error| VoiceInputError::ModelNotFound(error.to_string()))
}
