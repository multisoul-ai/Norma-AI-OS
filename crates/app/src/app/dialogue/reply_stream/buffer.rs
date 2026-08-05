use super::QueuedPlaybackChunk;
use crate::app::voice::playback::pcm_chunk_duration;
use std::collections::VecDeque;
use std::time::Duration;

/// Holds streamed PCM until enough audio exists to start a continuous reply.
pub(crate) struct StreamPlaybackBuffer {
    minimum_start_duration: Duration,
    buffered_duration: Duration,
    chunks: VecDeque<QueuedPlaybackChunk>,
}

impl StreamPlaybackBuffer {
    pub(crate) fn new(minimum_start_duration: Duration) -> Self {
        Self {
            minimum_start_duration,
            buffered_duration: Duration::ZERO,
            chunks: VecDeque::new(),
        }
    }

    /// Queues one chunk and releases the complete prebuffer once it is ready.
    pub(crate) fn push(&mut self, chunk: QueuedPlaybackChunk) -> Vec<QueuedPlaybackChunk> {
        self.buffered_duration = self
            .buffered_duration
            .saturating_add(pcm_chunk_duration(&chunk.pcm));
        self.chunks.push_back(chunk);
        if self.buffered_duration < self.minimum_start_duration {
            return Vec::new();
        }
        self.take_all()
    }

    /// Flushes audio after the TTS segment completes, even when it is short.
    pub(crate) fn finish(&mut self) -> Vec<QueuedPlaybackChunk> {
        self.take_all()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.chunks.clear();
        self.buffered_duration = Duration::ZERO;
    }

    fn take_all(&mut self) -> Vec<QueuedPlaybackChunk> {
        self.buffered_duration = Duration::ZERO;
        self.chunks.drain(..).collect()
    }
}
