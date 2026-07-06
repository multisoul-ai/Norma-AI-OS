//! Assistant audio playback sink used by realtime voice chat.

use realtime::PcmAudioChunk;

pub(crate) trait AssistantAudioSink: Send {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk);
    fn stop(&mut self);
}

#[derive(Default)]
pub(crate) struct NoopAssistantAudioSink;

impl AssistantAudioSink for NoopAssistantAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) {
        tracing::trace!(
            bytes = chunk.bytes.len(),
            sample_rate_hz = chunk.sample_rate_hz,
            channels = chunk.channels,
            "received assistant PCM chunk without a runtime playback backend"
        );
    }

    fn stop(&mut self) {
        tracing::trace!("stopped assistant PCM playback sink");
    }
}

pub(crate) fn default_assistant_audio_sink() -> Box<dyn AssistantAudioSink> {
    Box::<NoopAssistantAudioSink>::default()
}
