//! Isolated playback ownership for a Voice Settings TTS preview.

use super::reply::create_preview_audio_sink;
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::{PcmEnergyTimeline, TtsPlaybackEvent};
use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A preview has its own sink and cancellation control, so it never enters the assistant
/// reply queue or dialogue phase.
pub(crate) struct TtsPreviewControl {
    cancelled: Arc<AtomicBool>,
    sink: Mutex<Option<Box<dyn AssistantAudioSink>>>,
}

impl TtsPreviewControl {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            sink: Mutex::new(None),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut sink) = self.sink.try_lock() {
            if let Some(mut sink) = sink.take() {
                sink.stop();
                let _ = std::thread::Builder::new()
                    .name("tts_preview_audio_release".to_string())
                    .spawn(move || drop(sink));
            }
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    pub(crate) fn play_with_completion(
        &self,
        chunk: &PcmAudioChunk,
    ) -> Result<(async_channel::Receiver<()>, async_channel::Receiver<()>), String> {
        if self.is_cancelled() {
            return Err("Voice preview was cancelled".to_string());
        }
        let mut sink = create_preview_audio_sink();
        if self.is_cancelled() {
            sink.stop();
            return Err("Voice preview was cancelled".to_string());
        }
        let (started_tx, started_rx) = async_channel::bounded(1);
        let (finished_tx, finished_rx) = async_channel::bounded(1);
        if let Err(error) = sink.write_chunk_with_completion(chunk, started_tx, finished_tx) {
            sink.stop();
            return Err(error);
        }
        if self.is_cancelled() {
            sink.stop();
            return Err("Voice preview was cancelled".to_string());
        }
        let mut current = self
            .sink
            .lock()
            .map_err(|_| "Voice preview sink lock was poisoned".to_string())?;
        if self.is_cancelled() {
            sink.stop();
            return Err("Voice preview was cancelled".to_string());
        }
        if let Some(previous) = current.as_mut() {
            previous.stop();
        }
        *current = Some(sink);
        Ok((started_rx, finished_rx))
    }
}

#[derive(Default)]
pub(crate) struct TtsPreviewState {
    generation: u64,
    current: Option<Arc<TtsPreviewControl>>,
    playback_started: bool,
    playback_events: Vec<TtsPlaybackEvent>,
    pcm_queue_count: u64,
}

impl TtsPreviewState {
    pub(crate) fn start(&mut self) -> (u64, Arc<TtsPreviewControl>) {
        self.cancel();
        self.generation = self.generation.wrapping_add(1);
        let control = Arc::new(TtsPreviewControl::new());
        self.current = Some(Arc::clone(&control));
        (self.generation, control)
    }

    pub(crate) fn cancel(&mut self) {
        let had_current = self.current.is_some();
        if let Some(control) = self.current.take() {
            control.cancel();
        }
        self.playback_events.retain(|event| {
            !matches!(
                event,
                TtsPlaybackEvent::PcmQueued(_) | TtsPlaybackEvent::Started
            )
        });
        if self.playback_started {
            self.playback_events.push(TtsPlaybackEvent::Interrupted);
        } else if had_current {
            self.playback_events.push(TtsPlaybackEvent::DiscardQueued);
        }
        self.playback_started = false;
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
            && self
                .current
                .as_ref()
                .is_some_and(|control| !control.is_cancelled())
    }

    pub(crate) fn is_active(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|control| !control.is_cancelled())
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn pcm_queue_count(&self) -> u64 {
        self.pcm_queue_count
    }

    pub(crate) fn synchronize_external_cancellation(&mut self) {
        let externally_cancelled = self
            .current
            .as_ref()
            .is_some_and(|control| control.is_cancelled());
        if externally_cancelled {
            self.cancel();
        }
    }

    pub(crate) fn queue_pcm(&mut self, generation: u64, chunk: &PcmAudioChunk) {
        if self.is_current(generation) {
            self.pcm_queue_count = self.pcm_queue_count.wrapping_add(1);
            self.playback_events
                .push(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(
                    chunk,
                )));
        }
    }

    pub(crate) fn mark_playback_started(&mut self, generation: u64) {
        if self.is_current(generation) && !self.playback_started {
            self.playback_started = true;
            self.playback_events.push(TtsPlaybackEvent::Started);
        }
    }

    pub(crate) fn finish_playback(&mut self, generation: u64) {
        if !self.is_current(generation) {
            return;
        }
        self.current = None;
        if self.playback_started {
            self.playback_events.push(TtsPlaybackEvent::Finished);
        }
        self.playback_started = false;
    }

    pub(crate) fn has_pending_playback_events(&self) -> bool {
        !self.playback_events.is_empty()
    }

    pub(crate) fn take_playback_events(&mut self) -> Vec<TtsPlaybackEvent> {
        std::mem::take(&mut self.playback_events)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/tts_preview_tests.rs"]
mod tests;
