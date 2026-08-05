//! Streaming reply state shared by the dialogue controller and playback bridge.

use super::DialogueControlState;
use crate::app::voice::playback::pcm_chunk_duration;
use dialogue::DialoguePhase;
use gpui::Context;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "reply_stream/buffer.rs"]
mod buffer;
#[path = "reply_stream/capture.rs"]
pub(super) mod capture;
#[path = "reply_stream/pcm.rs"]
mod pcm;
#[path = "reply_stream/runtime.rs"]
mod runtime;
#[path = "reply_stream/scheduling.rs"]
mod scheduling;
#[path = "reply_stream/trace.rs"]
mod trace;

pub(super) use buffer::StreamPlaybackBuffer;
#[cfg(test)]
pub(super) use pcm::pcm_trace_fingerprint;
pub(super) use pcm::QueuedPlaybackChunk;

static NEXT_TTS_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

enum SpeechWorkerEvent {
    Chunk(QueuedPlaybackChunk),
    SegmentConsumed(dialogue::SpeechSegmentReceipt),
    Finished(Result<tts::TtsStreamSummary, tts::TtsError>),
}

pub(super) enum SpeechCommand {
    Segment {
        text: String,
        receipt: dialogue::SpeechSegmentReceipt,
    },
    Finish,
}

// Eight commands bound the background handoff. The Dialogue-domain SpeechBacklog separately
// bounds all pending plus submitted-but-unconsumed text to 10,000 active characters.
const SPEECH_COMMAND_CAPACITY: usize = 8;
// 16 PCM events allow two chunks per command slot; a faster producer is cancelled instead of
// growing memory without bound, while the terminal result uses a separate one-value handoff.
const SPEECH_EVENT_CAPACITY: usize = 16;

impl DialogueControlState {
    pub(crate) fn connect_playback_event_sink(
        &mut self,
        sink: crate::app::voice_window_inset::VoiceWindowInsetEventSink,
    ) {
        self.playback_event_sink = Some(sink);
    }

    pub(super) fn push_playback_event(
        &mut self,
        event: crate::app::voice_window_inset::TtsPlaybackEvent,
    ) {
        if let Some(sink) = &self.playback_event_sink {
            sink.push(event);
        } else {
            self.playback_events.push(event);
        }
    }

    pub(crate) fn begin_reply_stream(&mut self) {
        self.cancel_tts_preview();
        self.abort_reply_stream(false);
        self.tts_cancelled = Arc::new(AtomicBool::new(false));
        self.utterance_committer = dialogue::UtteranceCommitter::new();
        self.speech_backlog = dialogue::SpeechBacklog::default();
        self.speech_commands = None;
        self.speech_finish_queued = false;
        self.speech_request_sequence = 0;
        self.playback_buffer.clear();
        self.speech_synthesis_active = false;
        self.dialogue_stream_finished = false;
        self.playback_chunks_pending = 0;
        self.playback_started = false;
        self.reply_stream_started_at = Some(Instant::now());
        self.last_reply_text = Some(String::new());
        self.last_reply_interrupted = false;
        self.pcm_capture = capture::TtsPcmCapture::start(self.reply_generation);
        trace::reply_stream_started(self.reply_generation);
    }

    pub(crate) fn begin_static_reply_stream(
        &mut self,
        display_text: String,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        self.begin_reply_stream();
        self.on_agent_text_delta(&display_text, cx);
        self.finish_agent_text_stream(cx);
    }

    pub(super) fn finish_silent_reply(&mut self, display_text: String) {
        self.begin_reply_stream();
        self.last_reply_text = Some(display_text);
        self.dialogue_stream_finished = true;
        self.finish_reply_stream_if_ready();
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
        trace::dialogue_delta(
            self.reply_generation,
            text,
            self.last_reply_text
                .as_ref()
                .map_or(0, |reply| reply.chars().count()),
        );
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
        trace::dialogue_finished(
            self.reply_generation,
            self.last_reply_text.as_deref().unwrap_or_default(),
            final_segments.len(),
        );
        self.enqueue_speech_segments(final_segments, cx);
        self.speech_backlog.close_input();
        self.drive_speech_stream(cx);
        self.finish_reply_stream_if_ready();
    }

    pub(super) fn abort_reply_stream(&mut self, mark_interrupted: bool) {
        self.pcm_capture.finish();
        trace::reply_stream_aborted(trace::ReplyStreamAbortTrace {
            reply_generation: self.reply_generation,
            mark_interrupted,
            queued_segments: self.speech_backlog.pending_len(),
            playback_chunks_pending: self.playback_chunks_pending,
            cumulative_accepted_chars: self.speech_backlog.cumulative_accepted_chars(),
            cumulative_submitted_chars: self.speech_backlog.cumulative_submitted_chars(),
            cumulative_consumed_chars: self.speech_backlog.cumulative_consumed_chars(),
            active_chars_high_water: self.speech_backlog.active_chars_high_water(),
        });
        self.playback_events.retain(|event| {
            !matches!(
                event,
                crate::app::voice_window_inset::TtsPlaybackEvent::PcmQueued(_)
                    | crate::app::voice_window_inset::TtsPlaybackEvent::Started
            )
        });
        if self.playback_started {
            self.push_playback_event(crate::app::voice_window_inset::TtsPlaybackEvent::Interrupted);
        } else {
            self.push_playback_event(
                crate::app::voice_window_inset::TtsPlaybackEvent::DiscardQueued,
            );
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
        self.speech_commands = None;
        self.speech_finish_queued = false;
        self.speech_request_sequence = 0;
        let capacity_exceeded = self.speech_backlog.capacity_exceeded();
        self.speech_backlog.cancel();
        self.speech_backlog = dialogue::SpeechBacklog::default();
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
        if mark_interrupted && capacity_exceeded {
            self.error_message = None;
        }
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
                trace::pcm_received(generation, &chunk, self.stream_elapsed());
                let ready_chunks = self.playback_buffer.push(chunk);
                self.schedule_audio_chunks(ready_chunks, generation, cx);
                true
            }
            SpeechWorkerEvent::SegmentConsumed(receipt) => {
                if let Err(error) = self.speech_backlog.acknowledge_consumed(receipt) {
                    self.recover_reply_failure(format!("Speech error: {error}"));
                    return false;
                }
                self.drive_speech_stream(cx);
                true
            }
            SpeechWorkerEvent::Finished(result) => {
                trace::synthesis_finished(
                    generation,
                    &result,
                    self.stream_elapsed(),
                    self.speech_backlog.cumulative_accepted_chars(),
                    self.speech_backlog.cumulative_submitted_chars(),
                    self.speech_backlog.cumulative_consumed_chars(),
                    self.speech_backlog.active_chars_high_water(),
                );
                self.speech_synthesis_active = false;
                self.speech_task = None;
                self.speech_commands = None;
                self.speech_finish_queued = false;
                let final_chunks = self.playback_buffer.finish();
                self.schedule_audio_chunks(final_chunks, generation, cx);
                if let Err(error) = result {
                    self.recover_reply_failure(format!("Speech error: {error}"));
                    return false;
                }
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
            let pcm_duration = pcm_chunk_duration(&chunk.pcm);
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
            self.pcm_capture.append(&chunk.pcm);
            trace::playback_accepted(generation, &chunk, pcm_duration);
            self.push_playback_event(crate::app::voice_window_inset::TtsPlaybackEvent::PcmQueued(
                chunk.energy_timeline,
            ));
            self.playback_chunks_pending = self.playback_chunks_pending.saturating_add(1);
            let tts_request_sequence = chunk.tts_request_sequence;
            let chunk_sequence = chunk.chunk_sequence;
            let pcm_fingerprint = chunk.pcm_fingerprint;
            cx.spawn(async move |this, cx| {
                if playback_started_rx.recv().await.is_ok()
                    && this
                        .update(cx, |app, cx| {
                            let visible_state_changed = app.dialogue.on_playback_chunk_started(
                                generation,
                                tts_request_sequence,
                                chunk_sequence,
                                pcm_fingerprint,
                            );
                            if visible_state_changed {
                                cx.notify();
                            }
                        })
                        .is_err()
                {
                    return;
                }
                if completion_rx.recv().await.is_ok() {
                    let _ = this.update(cx, |app, cx| {
                        let visible_state_changed = app.dialogue.on_playback_chunk_finished(
                            generation,
                            tts_request_sequence,
                            chunk_sequence,
                            pcm_fingerprint,
                        );
                        if visible_state_changed {
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
        }
    }

    fn on_playback_chunk_started(
        &mut self,
        generation: u64,
        tts_request_sequence: u64,
        chunk_sequence: u64,
        pcm_fingerprint: u64,
    ) -> bool {
        if generation != self.reply_generation {
            return false;
        }
        trace::playback_started(
            generation,
            tts_request_sequence,
            chunk_sequence,
            pcm_fingerprint,
        );
        if self.playback_started {
            return false;
        }
        self.playback_started = true;
        self.push_playback_event(crate::app::voice_window_inset::TtsPlaybackEvent::Started);
        true
    }

    pub(super) fn on_playback_chunk_finished(
        &mut self,
        generation: u64,
        tts_request_sequence: u64,
        chunk_sequence: u64,
        pcm_fingerprint: u64,
    ) -> bool {
        if generation != self.reply_generation {
            return false;
        }
        trace::playback_finished(
            generation,
            tts_request_sequence,
            chunk_sequence,
            pcm_fingerprint,
        );
        self.playback_chunks_pending = self.playback_chunks_pending.saturating_sub(1);
        if self.finish_reply_stream_if_ready() && self.playback_started {
            self.playback_started = false;
            self.push_playback_event(crate::app::voice_window_inset::TtsPlaybackEvent::Finished);
            return true;
        }
        false
    }

    fn finish_reply_stream_if_ready(&mut self) -> bool {
        if !self.dialogue_stream_finished
            || self.speech_synthesis_active
            || !self.speech_backlog.is_empty()
            || !self.playback_buffer.is_empty()
            || self.playback_chunks_pending != 0
        {
            return false;
        }
        self.session
            .on_playback_finished_or_interrupted(self.reply_keeps_listening);
        self.pcm_capture.finish();
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
