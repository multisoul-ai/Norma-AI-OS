use audio::{PcmAudioChunk, PcmFormat};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use terminal::{PtyBackend, TestPtyBackend};

pub(super) fn echo_pty_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum GatedTtsCall {
    Start,
    Push(String),
    Finish,
    Cancel,
}

pub(super) struct GatedTtsClient {
    calls: Arc<Mutex<Vec<GatedTtsCall>>>,
    permits: mpsc::Receiver<()>,
    sink: Option<Arc<dyn tts::TtsChunkSink>>,
    pushed_chars: u64,
    chunk_count: u64,
}

impl GatedTtsClient {
    pub(super) fn new(calls: Arc<Mutex<Vec<GatedTtsCall>>>, permits: mpsc::Receiver<()>) -> Self {
        Self {
            calls,
            permits,
            sink: None,
            pushed_chars: 0,
            chunk_count: 0,
        }
    }

    fn record(&self, call: GatedTtsCall) {
        self.calls
            .lock()
            .expect("gated TTS call log lock should not be poisoned")
            .push(call);
    }
}

impl tts::TtsClient for GatedTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        self.record(GatedTtsCall::Start);
        self.sink = Some(sink);
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        self.record(GatedTtsCall::Push(text.to_string()));
        let sink = Arc::clone(
            self.sink
                .as_ref()
                .expect("gated TTS stream must start before text is pushed"),
        );
        // 5s = 50 × 100ms scheduling quanta; tests release permits immediately after observing
        // backlog, so this is only a deterministic deadlock ceiling for a failed implementation.
        let mut permitted = false;
        for _ in 0..50 {
            if sink.is_cancelled() {
                return Err(tts::TtsError::new(
                    tts::TtsErrorKind::Cancelled,
                    "gated TTS sink was cancelled while waiting for a permit",
                ));
            }
            match self.permits.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => {
                    permitted = true;
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if !permitted {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Transport,
                "gated TTS test permit timed out",
            ));
        }
        let pcm = PcmAudioChunk::new(
            PcmFormat::Pcm16,
            16_000,
            1,
            // 3,840 bytes / 2 bytes per sample / 16,000 Hz = 120ms, exactly the playback buffer.
            vec![0; 3_840],
        )
        .expect("gated TTS fixture must be valid 120ms PCM16 mono");
        if !sink.send(pcm) {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "gated TTS sink was cancelled",
            ));
        }
        self.pushed_chars = self
            .pushed_chars
            .saturating_add(text.chars().count() as u64);
        self.chunk_count = self.chunk_count.saturating_add(1);
        Ok(())
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        self.record(GatedTtsCall::Finish);
        let sink = self
            .sink
            .take()
            .expect("gated TTS stream must start before finish");
        sink.finish(Ok(tts::TtsStreamSummary::new(
            None,
            None,
            self.pushed_chars,
            self.chunk_count,
            self.chunk_count.saturating_mul(3_840),
        )));
        Ok(())
    }

    fn cancel_stream(&mut self) {
        self.record(GatedTtsCall::Cancel);
        if let Some(sink) = self.sink.take() {
            sink.finish(Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "gated TTS stream was cancelled",
            )));
        }
    }
}
