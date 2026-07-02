use crate::session::{AudioQueueStats, PcmEvent, TAP_DRAIN_WAIT_MS};
use crate::SpeechError;
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct SpeechFinalizer {
    thread_handle: Option<std::thread::JoinHandle<String>>,
    pcm_tx: Option<crossbeam_channel::Sender<PcmEvent>>,
    abort_tx: crossbeam_channel::Sender<()>,
    stats: Arc<AudioQueueStats>,
}

impl SpeechFinalizer {
    pub(crate) fn new(
        thread_handle: Option<std::thread::JoinHandle<String>>,
        pcm_tx: Option<crossbeam_channel::Sender<PcmEvent>>,
        abort_tx: crossbeam_channel::Sender<()>,
        stats: Arc<AudioQueueStats>,
    ) -> Self {
        Self {
            thread_handle,
            pcm_tx,
            abort_tx,
            stats,
        }
    }

    pub(crate) fn finish(mut self) -> Result<String, SpeechError> {
        std::thread::sleep(Duration::from_millis(TAP_DRAIN_WAIT_MS));
        self.finish_pcm_input();
        match self.thread_handle.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| SpeechError::Recognizer("recognizer thread panicked".to_string()))
                .and_then(|s| {
                    if s.is_empty() {
                        Err(SpeechError::EmptyTranscript)
                    } else {
                        Ok(s)
                    }
                }),
            None => Err(SpeechError::EmptyTranscript),
        }
    }

    fn finish_pcm_input(&mut self) {
        let Some(tx) = self.pcm_tx.take() else {
            return;
        };
        match tx.try_send(PcmEvent::EndOfAudio) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                tracing::warn!(
                    target: "voice_sherpa::session",
                    sent_chunks = self.stats.sent_chunks(),
                    dropped_chunks = self.stats.dropped_chunks(),
                    "voice PCM queue full while sending EndOfAudio; using channel disconnect as graceful end"
                );
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {}
        }
    }
}

impl Drop for SpeechFinalizer {
    fn drop(&mut self) {
        if self.thread_handle.is_some() {
            let _ = self.abort_tx.try_send(());
        }
        self.pcm_tx.take();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
