//! Streaming reply state shared by the dialogue controller and playback bridge.

use super::reply::{create_tts_client, selected_tts_model_configured, TtsClientBox};
use super::DialogueControlState;
use crate::app::voice::playback::pcm_chunk_duration;
use crate::app::voice_window_inset::PcmEnergyTimeline;
use audio::PcmAudioChunk;
use dialogue::{DialoguePhase, TtsChunkSink, SHORT_SPEECH_MERGE_WINDOW};
use gpui::Context;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use voice_sherpa::SpeechError;

/// Holds streamed PCM until enough audio exists to start a continuous reply.
pub(super) struct StreamPlaybackBuffer {
    minimum_start_duration: Duration,
    buffered_duration: Duration,
    chunks: VecDeque<QueuedPlaybackChunk>,
}

enum SpeechWorkerEvent {
    Chunk(QueuedPlaybackChunk),
    Finished {
        tts: Option<TtsClientBox>,
        result: Result<(), SpeechError>,
    },
}

pub(super) struct QueuedPlaybackChunk {
    pub(super) pcm: PcmAudioChunk,
    energy_timeline: PcmEnergyTimeline,
}

impl QueuedPlaybackChunk {
    pub(super) fn from_pcm(pcm: PcmAudioChunk) -> Self {
        let energy_timeline = PcmEnergyTimeline::from_pcm(&pcm);
        Self {
            pcm,
            energy_timeline,
        }
    }
}

struct ChannelTtsChunkSink {
    events: async_channel::Sender<SpeechWorkerEvent>,
    cancelled: Arc<AtomicBool>,
}

impl TtsChunkSink for ChannelTtsChunkSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && self
                .events
                .try_send(SpeechWorkerEvent::Chunk(QueuedPlaybackChunk::from_pcm(
                    chunk,
                )))
                .is_ok()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl DialogueControlState {
    pub(super) fn begin_reply_stream(&mut self) {
        self.abort_reply_stream(false);
        self.tts_cancelled = Arc::new(AtomicBool::new(false));
        self.utterance_committer = dialogue::UtteranceCommitter::new();
        self.speech_queue.clear();
        self.playback_buffer.clear();
        self.speech_synthesis_active = false;
        self.dialogue_stream_finished = false;
        self.playback_chunks_pending = 0;
        self.playback_started = false;
        self.reply_stream_started_at = Some(Instant::now());
        self.last_reply_text = Some(String::new());
        self.last_reply_interrupted = false;
    }

    pub(super) fn begin_static_reply_stream(
        &mut self,
        display_text: String,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        self.begin_reply_stream();
        self.on_agent_text_delta(&display_text, cx);
        self.finish_agent_text_stream(cx);
    }

    pub(crate) fn on_agent_text_delta(
        &mut self,
        text: &str,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        if text.is_empty() || self.session.phase() == DialoguePhase::Listening {
            return;
        }
        self.last_reply_text
            .get_or_insert_with(String::new)
            .push_str(text);
        let segments = self
            .utterance_committer
            .push_delta(text, self.stream_elapsed());
        self.enqueue_speech_segments(segments, cx);
        self.schedule_merge_window_check(cx);
    }

    pub(crate) fn finish_agent_text_stream(&mut self, cx: &mut Context<crate::app::MultisoulApp>) {
        if self.session.phase() == DialoguePhase::Listening {
            return;
        }
        self.dialogue_stream_finished = true;
        let final_segments = self.utterance_committer.finish();
        self.enqueue_speech_segments(final_segments, cx);
        self.finish_reply_stream_if_ready();
    }

    pub(super) fn abort_reply_stream(&mut self, mark_interrupted: bool) {
        self.playback_events
            .retain(|event| !matches!(event, super::AssistantPlaybackEvent::PcmQueued(_)));
        if self.playback_started {
            self.playback_events
                .push(super::AssistantPlaybackEvent::Interrupted);
        }
        self.reply_generation = self.reply_generation.wrapping_add(1);
        self.tts_cancelled.store(true, Ordering::Release);
        if mark_interrupted {
            if let Some(cancellation) = self.dialogue_stream_cancellation.as_ref() {
                cancellation.store(true, Ordering::Release);
            }
        }
        self.reply_playback.interrupt();
        self.speech_task = None;
        self.speech_queue.clear();
        self.playback_buffer.clear();
        self.speech_synthesis_active = false;
        self.playback_chunks_pending = 0;
        self.playback_started = false;
        self.dialogue_stream_finished = false;
        self.utterance_committer = dialogue::UtteranceCommitter::new();
        self.reply_stream_started_at = None;
        if mark_interrupted && self.last_reply_text.is_some() {
            self.last_reply_interrupted = true;
        }
    }

    fn enqueue_speech_segments(
        &mut self,
        segments: Vec<String>,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        self.speech_queue.extend(segments);
        self.start_next_speech_synthesis(cx);
    }

    fn schedule_merge_window_check(&self, cx: &mut Context<crate::app::MultisoulApp>) {
        let generation = self.reply_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SHORT_SPEECH_MERGE_WINDOW)
                .await;
            let _ = this.update(cx, |app, cx| {
                app.dialogue.on_merge_window_elapsed(generation, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn on_merge_window_elapsed(
        &mut self,
        generation: u64,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        if generation != self.reply_generation || self.dialogue_stream_finished {
            return;
        }
        let segments = self.utterance_committer.on_idle(self.stream_elapsed());
        self.enqueue_speech_segments(segments, cx);
    }

    fn start_next_speech_synthesis(&mut self, cx: &mut Context<crate::app::MultisoulApp>) {
        if self.speech_synthesis_active || self.speech_queue.is_empty() {
            return;
        }
        if !selected_tts_model_configured() {
            self.speech_queue.clear();
            self.finish_reply_stream_if_ready();
            return;
        }
        let generation = self.reply_generation;
        let text = self
            .speech_queue
            .pop_front()
            .expect("speech queue was checked before starting synthesis");
        let existing_tts = self.tts_client.take();
        let cancelled = Arc::clone(&self.tts_cancelled);
        let (events_tx, events_rx) = async_channel::unbounded();
        self.speech_synthesis_active = true;
        let task = cx.spawn(async move |this, cx| {
            let worker_events = events_tx.clone();
            cx.background_executor()
                .spawn(async move {
                    let mut tts = match existing_tts {
                        Some(tts) => tts,
                        None => match create_tts_client() {
                            Ok(tts) => tts,
                            Err(error) => {
                                let _ = worker_events
                                    .send(SpeechWorkerEvent::Finished {
                                        tts: None,
                                        result: Err(error),
                                    })
                                    .await;
                                return;
                            }
                        },
                    };
                    let sink = Arc::new(ChannelTtsChunkSink {
                        events: worker_events.clone(),
                        cancelled,
                    });
                    let result = tts
                        .synthesize_stream(&text, sink)
                        .map_err(|error| SpeechError::Recognizer(error.to_string()));
                    let _ = worker_events
                        .send(SpeechWorkerEvent::Finished {
                            tts: Some(tts),
                            result,
                        })
                        .await;
                })
                .detach();
            while let Ok(event) = events_rx.recv().await {
                let keep_receiving = this.update(cx, |app, cx| {
                    app.dialogue.on_speech_worker_event(generation, event, cx)
                });
                if !matches!(keep_receiving, Ok(true)) {
                    break;
                }
            }
        });
        self.speech_task = Some(task);
    }

    fn on_speech_worker_event(
        &mut self,
        generation: u64,
        event: SpeechWorkerEvent,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) -> bool {
        if generation != self.reply_generation {
            return false;
        }
        match event {
            SpeechWorkerEvent::Chunk(chunk) => {
                let ready_chunks = self.playback_buffer.push(chunk);
                self.schedule_audio_chunks(ready_chunks, generation, cx);
                true
            }
            SpeechWorkerEvent::Finished { tts, result } => {
                self.tts_client = tts;
                self.speech_synthesis_active = false;
                self.speech_task = None;
                let final_chunks = self.playback_buffer.finish();
                self.schedule_audio_chunks(final_chunks, generation, cx);
                if let Err(error) = result {
                    self.recover_reply_failure(format!("Speech error: {error}"));
                    return false;
                }
                self.start_next_speech_synthesis(cx);
                self.finish_reply_stream_if_ready();
                false
            }
        }
    }

    fn schedule_audio_chunks(
        &mut self,
        chunks: Vec<QueuedPlaybackChunk>,
        generation: u64,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        for chunk in chunks {
            if self.session.phase() == DialoguePhase::Thinking
                && self.session.on_reply_ready().is_err()
            {
                self.recover_reply_failure("Speech error: invalid reply phase".to_string());
                return;
            }
            if self.session.phase() != DialoguePhase::Speaking {
                self.recover_reply_failure("Speech error: reply is no longer active".to_string());
                return;
            }
            let (playback_started_tx, playback_started_rx) = async_channel::bounded(1);
            let (completion_tx, completion_rx) = async_channel::bounded(1);
            if let Err(error) = self.reply_playback.play_chunk_with_completion(
                &chunk.pcm,
                playback_started_tx,
                completion_tx,
            ) {
                self.recover_reply_failure(format!("Speech error: {error}"));
                return;
            }
            self.playback_events
                .push(super::AssistantPlaybackEvent::PcmQueued(
                    chunk.energy_timeline,
                ));
            self.playback_chunks_pending = self.playback_chunks_pending.saturating_add(1);
            cx.spawn(async move |this, cx| {
                if playback_started_rx.recv().await.is_ok()
                    && this
                        .update(cx, |app, _cx| {
                            app.dialogue.on_playback_chunk_started(generation);
                        })
                        .is_err()
                {
                    return;
                }
                if completion_rx.recv().await.is_ok() {
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.on_playback_chunk_finished(generation);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    }

    fn on_playback_chunk_started(&mut self, generation: u64) {
        if generation != self.reply_generation || self.playback_started {
            return;
        }
        self.playback_started = true;
        self.playback_events
            .push(super::AssistantPlaybackEvent::Started);
    }

    fn on_playback_chunk_finished(&mut self, generation: u64) {
        if generation != self.reply_generation {
            return;
        }
        self.playback_chunks_pending = self.playback_chunks_pending.saturating_sub(1);
        if self.finish_reply_stream_if_ready() && self.playback_started {
            self.playback_started = false;
            self.playback_events
                .push(super::AssistantPlaybackEvent::Finished);
        }
    }

    fn finish_reply_stream_if_ready(&mut self) -> bool {
        if !self.dialogue_stream_finished
            || self.speech_synthesis_active
            || !self.speech_queue.is_empty()
            || !self.playback_buffer.is_empty()
            || self.playback_chunks_pending != 0
        {
            return false;
        }
        self.session
            .on_playback_finished_or_interrupted(self.reply_keeps_listening);
        self.reply_stream_started_at = None;
        self.dialogue_stream_finished = false;
        true
    }

    fn stream_elapsed(&self) -> Duration {
        self.reply_stream_started_at
            .map(|started| started.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}

impl StreamPlaybackBuffer {
    pub(super) fn new(minimum_start_duration: Duration) -> Self {
        Self {
            minimum_start_duration,
            buffered_duration: Duration::ZERO,
            chunks: VecDeque::new(),
        }
    }

    /// Queues one chunk and releases the complete prebuffer once it is ready.
    pub(super) fn push(&mut self, chunk: QueuedPlaybackChunk) -> Vec<QueuedPlaybackChunk> {
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
    pub(super) fn finish(&mut self) -> Vec<QueuedPlaybackChunk> {
        self.take_all()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.chunks.clear();
        self.buffered_duration = Duration::ZERO;
    }

    fn take_all(&mut self) -> Vec<QueuedPlaybackChunk> {
        self.buffered_duration = Duration::ZERO;
        self.chunks.drain(..).collect()
    }
}
