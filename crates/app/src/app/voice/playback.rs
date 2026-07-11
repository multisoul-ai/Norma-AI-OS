//! Assistant audio playback sink used by dialogue voice chat.

use audio::PcmAudioChunk;
#[cfg(test)]
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(target_os = "macos")]
mod macos;

pub(crate) trait AssistantAudioSink: Send {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String>;
    fn stop(&mut self);
}

#[derive(Default)]
pub(crate) struct NoopAssistantAudioSink;

impl AssistantAudioSink for NoopAssistantAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        tracing::trace!(
            bytes = chunk.bytes.len(),
            sample_rate_hz = chunk.sample_rate_hz,
            channels = chunk.channels,
            "received assistant PCM chunk without a runtime playback backend"
        );
        Err("assistant audio playback backend is unavailable".to_string())
    }

    fn stop(&mut self) {
        tracing::trace!("stopped assistant PCM playback sink");
    }
}

/// Records write/stop calls for unit tests.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct RecordingAssistantAudioSink {
    inner: Arc<Mutex<RecordingAssistantAudioSinkState>>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct RecordingAssistantAudioSinkState {
    chunks_written: usize,
    stopped: bool,
}

#[cfg(test)]
impl RecordingAssistantAudioSink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn chunks_written(&self) -> usize {
        self.inner
            .lock()
            .expect("recording sink lock should not be poisoned")
            .chunks_written
    }

    pub(crate) fn stopped(&self) -> bool {
        self.inner
            .lock()
            .expect("recording sink lock should not be poisoned")
            .stopped
    }
}

#[cfg(test)]
impl AssistantAudioSink for RecordingAssistantAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        let mut state = self
            .inner
            .lock()
            .expect("recording sink lock should not be poisoned");
        state.chunks_written = state.chunks_written.saturating_add(1);
        tracing::trace!(
            bytes = chunk.bytes.len(),
            sample_rate_hz = chunk.sample_rate_hz,
            channels = chunk.channels,
            "recording assistant PCM chunk"
        );
        Ok(pcm_chunk_duration(chunk))
    }

    fn stop(&mut self) {
        self.inner
            .lock()
            .expect("recording sink lock should not be poisoned")
            .stopped = true;
    }
}

pub(crate) fn default_assistant_audio_sink() -> Box<dyn AssistantAudioSink> {
    #[cfg(target_os = "macos")]
    {
        match macos::MacosAssistantAudioSink::new() {
            Ok(sink) => return Box::new(sink),
            Err(error) => {
                tracing::warn!(%error, "failed to create macOS assistant audio sink");
            }
        }
    }
    Box::<NoopAssistantAudioSink>::default()
}

pub(crate) fn interrupt_assistant_playback(sink: &mut dyn AssistantAudioSink) {
    sink.stop();
}

pub(crate) fn pcm_chunk_duration(chunk: &PcmAudioChunk) -> Duration {
    let frame_count = chunk.bytes.len() / 2 / usize::from(chunk.channels);
    Duration::from_secs_f64(frame_count as f64 / f64::from(chunk.sample_rate_hz))
}

#[cfg(test)]
#[path = "../../../tests/unit/playback_tests.rs"]
mod playback_tests;
