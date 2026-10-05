//! Provider-neutral speech backlog scheduling and bounded worker handoff.

use super::{trace, SpeechCommand};
use crate::app::dialogue::{spoken_reply_capacity_message, DialogueControlState};
use dialogue::SHORT_SPEECH_MERGE_WINDOW;
use gpui::Context;

impl DialogueControlState {
    pub(super) fn enqueue_speech_segments(
        &mut self,
        segments: Vec<String>,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        if self.speech_backlog.capacity_exceeded() {
            return;
        }
        let segment_chars = segments
            .iter()
            .map(|segment| segment.chars().count())
            .collect::<Vec<_>>();
        if segment_chars
            .iter()
            .any(|characters| *characters > dialogue::MAX_SPEECH_SEGMENT_CHARS)
        {
            self.recover_reply_failure(ui::i18n::LocalizedText::new(
                "chrome.reply_segment_too_large",
            ));
            return;
        }
        match self.speech_backlog.accept_batch(segments) {
            Ok(()) => {
                for characters in segment_chars {
                    trace::speech_segment_queued(self.reply_generation, characters);
                }
            }
            Err(dialogue::SpeechBacklogError::CapacityExceeded {
                active_chars,
                incoming_chars,
                capacity_chars,
            }) => {
                trace::speech_capacity_exceeded(
                    self.reply_generation,
                    active_chars,
                    incoming_chars,
                    capacity_chars,
                );
                self.error_message = Some(spoken_reply_capacity_message(capacity_chars));
            }
            Err(
                dialogue::SpeechBacklogError::InputClosed | dialogue::SpeechBacklogError::Cancelled,
            ) => return,
            Err(error) => {
                self.recover_reply_failure(
                    ui::i18n::LocalizedText::new("chrome.speech_error_error")
                        .with_arg("error", error),
                );
                return;
            }
        }
        self.drive_speech_stream(cx);
    }

    pub(super) fn schedule_merge_window_check(&self, cx: &mut Context<crate::app::MultisoulApp>) {
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

    pub(super) fn drive_speech_stream(&mut self, cx: &mut Context<crate::app::MultisoulApp>) {
        if self.speech_backlog.next_submission().is_none() && !self.speech_synthesis_active {
            self.finish_reply_stream_if_ready();
            return;
        }
        if self.reply_tts_selection().is_none() {
            self.speech_backlog.cancel();
            self.speech_backlog = dialogue::SpeechBacklog::default();
            self.finish_reply_stream_if_ready();
            return;
        }
        if !self.speech_synthesis_active {
            self.start_speech_worker(cx);
        }
        if let Err(error) = self.flush_speech_commands() {
            self.recover_reply_failure(
                ui::i18n::LocalizedText::new("chrome.speech_error_error").with_arg("error", error),
            );
        }
    }

    pub(crate) fn flush_speech_commands(&mut self) -> Result<(), String> {
        let Some(commands) = self.speech_commands.clone() else {
            return Ok(());
        };
        while let Some(submission) = self.speech_backlog.next_submission() {
            let text = submission.text().to_string();
            let receipt = submission.receipt();
            match commands.try_send(SpeechCommand::Segment {
                text: text.clone(),
                receipt,
            }) {
                Ok(()) => {
                    self.speech_backlog
                        .mark_submitted(receipt)
                        .map_err(|error| error.to_string())?;
                    trace::synthesis_requested(
                        self.reply_generation,
                        self.speech_request_sequence,
                        &text,
                        self.speech_backlog.pending_len(),
                    );
                }
                Err(async_channel::TrySendError::Full(_)) => return Ok(()),
                Err(async_channel::TrySendError::Closed(_)) => {
                    return Err("TTS command channel closed before the reply drained".to_string())
                }
            }
        }
        if self.speech_backlog.ready_to_finish() && !self.speech_finish_queued {
            match commands.try_send(SpeechCommand::Finish) {
                Ok(()) => self.speech_finish_queued = true,
                Err(async_channel::TrySendError::Full(_)) => {}
                Err(async_channel::TrySendError::Closed(_)) => {
                    return Err("TTS command channel closed before the reply drained".to_string())
                }
            }
        }
        Ok(())
    }
}
