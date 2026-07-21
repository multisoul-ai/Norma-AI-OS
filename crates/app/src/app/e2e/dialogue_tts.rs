//! Instance-isolated TTS and playback seam for cross-crate reply-stream E2E tests.

use super::MultisoulApp;
use crate::app::dialogue::{ReplyPlayback, SelectedTtsModel, SharedTtsRuntime};
use crate::app::voice::playback::{pcm_chunk_duration, AssistantAudioSink};
use audio::{PcmAudioChunk, PcmFormat};
use gpui::Context;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplyTtsE2ePcmSnapshot {
    pub format: &'static str,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub byte_count: usize,
    pub sequence_marker: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReplyTtsE2eProbeSnapshot {
    pub starts: usize,
    pub pushes: Vec<String>,
    pub finishes: usize,
    pub cancels: usize,
    pub pcm_writes: Vec<ReplyTtsE2ePcmSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplyTtsE2eSnapshot {
    pub visible_reply: Option<String>,
    pub voice_label: Option<String>,
    pub phase: &'static str,
    pub active_speech_chars: usize,
}

pub struct ReplyTtsE2eProbe {
    state: Arc<Mutex<ReplyTtsE2eProbeSnapshot>>,
    permits: mpsc::Sender<()>,
}

impl ReplyTtsE2eProbe {
    pub fn release_segments(&self, count: usize) {
        for _ in 0..count {
            self.permits
                .send(())
                .expect("gated reply TTS permit receiver must remain connected");
        }
    }

    pub fn snapshot(&self) -> ReplyTtsE2eProbeSnapshot {
        self.state
            .lock()
            .expect("reply TTS e2e probe lock should not be poisoned")
            .clone()
    }
}

struct GatedReplyTtsClient {
    state: Arc<Mutex<ReplyTtsE2eProbeSnapshot>>,
    permits: mpsc::Receiver<()>,
    sink: Option<Arc<dyn tts::TtsChunkSink>>,
    pushed_chars: u64,
    chunk_count: u64,
}

impl tts::TtsClient for GatedReplyTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        {
            let mut state = self.state();
            state.starts = state.starts.saturating_add(1);
        }
        self.sink = Some(sink);
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        self.state().pushes.push(text.to_string());
        // 5s = 50 × 100ms scheduling quanta. The E2E releases permits after observing the
        // blocked backlog; this timeout only turns an implementation deadlock into a failure.
        self.permits
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| {
                tts::TtsError::new(
                    tts::TtsErrorKind::Transport,
                    "gated reply TTS e2e permit timed out",
                )
            })?;
        let sequence_marker = self.chunk_count.saturating_add(1) as u8;
        let pcm = PcmAudioChunk::new(
            PcmFormat::Pcm16,
            16_000,
            1,
            // 3,840 bytes / 2 bytes per sample / 16,000 Hz = 120ms playback-buffer quantum.
            vec![sequence_marker; 3_840],
        )
        .expect("reply TTS e2e fixture must be valid PCM16 mono");
        let sink = self
            .sink
            .as_ref()
            .expect("reply TTS stream must start before text is pushed");
        if !sink.send(pcm) {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "reply TTS e2e sink was cancelled",
            ));
        }
        self.pushed_chars = self
            .pushed_chars
            .saturating_add(text.chars().count() as u64);
        self.chunk_count = self.chunk_count.saturating_add(1);
        Ok(())
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        {
            let mut state = self.state();
            state.finishes = state.finishes.saturating_add(1);
        }
        let sink = self
            .sink
            .take()
            .expect("reply TTS stream must start before finish");
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
        {
            let mut state = self.state();
            state.cancels = state.cancels.saturating_add(1);
        }
        if let Some(sink) = self.sink.take() {
            sink.finish(Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "reply TTS e2e stream was cancelled",
            )));
        }
    }
}

impl GatedReplyTtsClient {
    fn state(&self) -> std::sync::MutexGuard<'_, ReplyTtsE2eProbeSnapshot> {
        self.state
            .lock()
            .expect("reply TTS e2e client state lock should not be poisoned")
    }
}

struct ImmediateReplyAudioSink {
    state: Arc<Mutex<ReplyTtsE2eProbeSnapshot>>,
}

impl AssistantAudioSink for ImmediateReplyAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        let mut state = self
            .state
            .lock()
            .expect("reply TTS e2e playback state lock should not be poisoned");
        state.pcm_writes.push(ReplyTtsE2ePcmSnapshot {
            format: match chunk.format {
                PcmFormat::Pcm16 => "pcm16",
            },
            sample_rate_hz: chunk.sample_rate_hz,
            channels: chunk.channels,
            byte_count: chunk.bytes.len(),
            sequence_marker: chunk.bytes.first().copied().unwrap_or_default(),
        });
        Ok(pcm_chunk_duration(chunk))
    }

    fn stop(&mut self) {}
}

impl MultisoulApp {
    pub fn install_gated_reply_tts_for_e2e(&mut self, cx: &mut Context<Self>) -> ReplyTtsE2eProbe {
        let selection = SelectedTtsModel::local(
            std::path::PathBuf::new(),
            tts::TtsModelId::parse("e2e-reply-backlog-model")
                .expect("reply TTS e2e model ID must be valid"),
            tts::TtsVoiceId::parse("e2e-reply-backlog-voice")
                .expect("reply TTS e2e voice ID must be valid"),
        );
        let state = Arc::new(Mutex::new(ReplyTtsE2eProbeSnapshot::default()));
        let (permit_tx, permit_rx) = mpsc::channel();
        let client = GatedReplyTtsClient {
            state: Arc::clone(&state),
            permits: permit_rx,
            sink: None,
            pushed_chars: 0,
            chunk_count: 0,
        };
        self.dialogue.reply_tts_selection_override = Some(selection.clone());
        self.dialogue.shared_tts_runtime =
            SharedTtsRuntime::from_client_for_tests(Some(selection), Box::new(client));
        self.dialogue.reply_playback = ReplyPlayback::new(Box::new(ImmediateReplyAudioSink {
            state: Arc::clone(&state),
        }));
        cx.notify();
        ReplyTtsE2eProbe {
            state,
            permits: permit_tx,
        }
    }

    pub fn begin_static_reply_for_e2e(&mut self, text: String, cx: &mut Context<Self>) {
        self.begin_reply_stream_for_e2e(cx);
        self.push_reply_delta_for_e2e(&text, cx);
        self.finish_reply_stream_for_e2e(cx);
    }

    pub fn begin_reply_stream_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.dialogue.session.on_listening_started();
        self.dialogue
            .session
            .on_utterance_final()
            .expect("reply TTS e2e fixture must transition Listening to Thinking");
        self.dialogue.begin_reply_stream();
        cx.notify();
    }

    pub fn push_reply_delta_for_e2e(&mut self, text: &str, cx: &mut Context<Self>) {
        self.dialogue.on_agent_text_delta(text, cx);
        cx.notify();
    }

    pub fn finish_reply_stream_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.dialogue.finish_agent_text_stream(cx);
        cx.notify();
    }

    pub fn reply_tts_e2e_snapshot(&self) -> ReplyTtsE2eSnapshot {
        ReplyTtsE2eSnapshot {
            visible_reply: self.dialogue.last_reply_text.clone(),
            voice_label: self.dialogue.dialogue_voice_label(),
            phase: match self.dialogue.phase() {
                dialogue::DialoguePhase::Idle => "idle",
                dialogue::DialoguePhase::Listening => "listening",
                dialogue::DialoguePhase::Thinking => "thinking",
                dialogue::DialoguePhase::Speaking => "speaking",
            },
            active_speech_chars: self.dialogue.speech_backlog.active_chars(),
        }
    }

    pub fn remove_reply_tts_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.dialogue.reply_tts_selection_override = None;
        self.dialogue.shared_tts_runtime = SharedTtsRuntime::default();
        cx.notify();
    }
}
