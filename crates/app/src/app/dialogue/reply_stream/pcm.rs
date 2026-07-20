use crate::app::voice_window_inset::PcmEnergyTimeline;
use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::SpeechWorkerEvent;

// The sink runs on the background synthesis worker. Polling every 2ms keeps cancellation
// responsive while applying bounded backpressure instead of blocking GPUI's foreground thread.
const BACKPRESSURE_POLL_INTERVAL: Duration = Duration::from_millis(2);

pub(crate) struct QueuedPlaybackChunk {
    pub(crate) pcm: PcmAudioChunk,
    pub(crate) tts_request_sequence: u64,
    pub(crate) chunk_sequence: u64,
    pub(crate) pcm_fingerprint: u64,
    pub(crate) energy_timeline: PcmEnergyTimeline,
}

impl QueuedPlaybackChunk {
    #[cfg(test)]
    pub(crate) fn from_pcm(pcm: PcmAudioChunk) -> Self {
        Self::from_tts_stream(pcm, 0, 0)
    }

    fn from_tts_stream(pcm: PcmAudioChunk, tts_request_sequence: u64, chunk_sequence: u64) -> Self {
        let energy_timeline = PcmEnergyTimeline::from_pcm(&pcm);
        let pcm_fingerprint = pcm_trace_fingerprint(&pcm);
        Self {
            pcm,
            tts_request_sequence,
            chunk_sequence,
            pcm_fingerprint,
            energy_timeline,
        }
    }
}

/// Stable, dependency-free fingerprint used only to correlate PCM blocks in diagnostic logs.
pub(crate) fn pcm_trace_fingerprint(chunk: &PcmAudioChunk) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in chunk
        .sample_rate_hz
        .to_le_bytes()
        .into_iter()
        .chain(chunk.channels.to_le_bytes())
        .chain(chunk.bytes.iter().copied())
    {
        hash ^= u64::from(value);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(super) struct ChannelTtsChunkSink {
    pub(super) events: async_channel::Sender<SpeechWorkerEvent>,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) tts_request_sequence: u64,
    pub(super) next_chunk_sequence: AtomicU64,
    pub(super) terminal: Mutex<Option<Result<tts::TtsStreamSummary, tts::TtsError>>>,
}

impl ChannelTtsChunkSink {
    pub(super) fn take_terminal(&self) -> Option<Result<tts::TtsStreamSummary, tts::TtsError>> {
        self.terminal
            .lock()
            .expect("TTS stream terminal lock should not be poisoned")
            .take()
    }
}

impl tts::TtsChunkSink for ChannelTtsChunkSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        let mut event = SpeechWorkerEvent::Chunk(QueuedPlaybackChunk::from_tts_stream(
            chunk,
            self.tts_request_sequence,
            self.next_chunk_sequence.fetch_add(1, Ordering::Relaxed),
        ));
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return false;
            }
            match self.events.try_send(event) {
                Ok(()) => return true,
                Err(async_channel::TrySendError::Full(returned)) => {
                    event = returned;
                    std::thread::park_timeout(BACKPRESSURE_POLL_INTERVAL);
                }
                Err(async_channel::TrySendError::Closed(_)) => return false,
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn finish(&self, result: Result<tts::TtsStreamSummary, tts::TtsError>) {
        *self
            .terminal
            .lock()
            .expect("TTS stream terminal lock should not be poisoned") = Some(result);
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/reply_stream_pcm_tests.rs"]
mod tests;
