//! Isolated playback ownership for a Voice Settings TTS preview.

use super::reply::create_preview_audio_sink;
use crate::app::voice::playback::AssistantAudioSink;
use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A preview has its own sink and cancellation control, so it never enters the assistant
/// reply queue, dialogue phase, or voice-window-inset timeline.
pub(crate) struct TtsPreviewControl {
    cancelled: AtomicBool,
    sink: Mutex<Option<Box<dyn AssistantAudioSink>>>,
}

impl TtsPreviewControl {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            sink: Mutex::new(None),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut sink) = self.sink.lock() {
            if let Some(sink) = sink.as_mut() {
                sink.stop();
            }
            *sink = None;
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn play(&self, chunk: &PcmAudioChunk) {
        if self.is_cancelled() {
            return;
        }
        let Ok(mut current) = self.sink.lock() else {
            return;
        };
        if self.is_cancelled() {
            return;
        }
        if let Some(sink) = current.as_mut() {
            sink.stop();
        }
        let mut sink = create_preview_audio_sink();
        if self.is_cancelled() {
            sink.stop();
            return;
        }
        if sink.write_chunk(chunk).is_ok() && !self.is_cancelled() {
            *current = Some(sink);
        } else {
            sink.stop();
        }
    }
}

#[derive(Default)]
pub(crate) struct TtsPreviewState {
    generation: u64,
    current: Option<Arc<TtsPreviewControl>>,
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
        if let Some(control) = self.current.take() {
            control.cancel();
        }
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
}
