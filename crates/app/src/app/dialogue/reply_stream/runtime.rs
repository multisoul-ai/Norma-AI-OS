use super::super::reply::{create_tts_client_for_selection, selected_tts_model, SelectedTtsModel};
use super::super::SharedTtsRuntime;
use super::pcm::ChannelTtsChunkSink;
use super::{
    trace, DialogueControlState, SpeechCommand, SpeechWorkerEvent, NEXT_TTS_REQUEST_SEQUENCE,
    SPEECH_COMMAND_CAPACITY, SPEECH_EVENT_CAPACITY,
};
use gpui::Context;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub(super) fn with_tts_client<R>(
    runtime: &SharedTtsRuntime,
    selected: Option<SelectedTtsModel>,
    operation: impl FnOnce(&mut dyn tts::TtsClient) -> Result<R, tts::TtsError>,
) -> Result<R, tts::TtsError> {
    let selected_for_creation = selected.clone();
    runtime.with_client(
        selected,
        || {
            create_tts_client_for_selection(selected_for_creation.as_ref()).map_err(|error| {
                tts::TtsError::new(tts::TtsErrorKind::Configuration, error.to_string())
            })
        },
        operation,
    )
}

impl DialogueControlState {
    pub(super) fn start_speech_worker(&mut self, cx: &mut Context<crate::app::MultisoulApp>) {
        let generation = self.reply_generation;
        let tts_request_sequence = NEXT_TTS_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tts_runtime = self.shared_tts_runtime.clone();
        let selected = selected_tts_model();
        if let Some(selection) = selected.as_ref() {
            trace::provider_stream_started(
                generation,
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            );
        }
        let cancelled = Arc::clone(&self.tts_cancelled);
        let (commands_tx, commands_rx) = async_channel::bounded(SPEECH_COMMAND_CAPACITY);
        let (events_tx, events_rx) = async_channel::bounded(SPEECH_EVENT_CAPACITY);
        self.speech_commands = Some(commands_tx);
        self.speech_finish_queued = false;
        self.speech_request_sequence = tts_request_sequence;
        self.speech_synthesis_active = true;
        let task = cx.spawn(async move |this, cx| {
            let worker_events = events_tx.clone();
            cx.background_executor()
                .spawn(async move {
                    let sink = Arc::new(ChannelTtsChunkSink {
                        events: worker_events.clone(),
                        cancelled,
                        tts_request_sequence,
                        next_chunk_sequence: AtomicU64::new(0),
                        terminal: std::sync::Mutex::new(None),
                    });
                    let start_result = with_tts_client(&tts_runtime, selected.clone(), |tts| {
                        tts.start_stream(sink.clone())
                    });
                    if let Err(error) = start_result {
                        let _ = worker_events
                            .send(SpeechWorkerEvent::Finished(Err(error)))
                            .await;
                        return;
                    }
                    while let Ok(command) = commands_rx.recv().await {
                        match command {
                            SpeechCommand::Segment(text) => {
                                if let Err(error) =
                                    with_tts_client(&tts_runtime, selected.clone(), |tts| {
                                        tts.push_text(&text)
                                    })
                                {
                                    let terminal = sink.take_terminal().unwrap_or(Err(error));
                                    let _ = worker_events
                                        .send(SpeechWorkerEvent::Finished(terminal))
                                        .await;
                                    return;
                                }
                                let _ =
                                    worker_events.send(SpeechWorkerEvent::CommandConsumed).await;
                            }
                            SpeechCommand::Finish => {
                                let result =
                                    with_tts_client(&tts_runtime, selected.clone(), |tts| {
                                        tts.finish_stream()
                                    });
                                let terminal =
                                    sink.take_terminal().unwrap_or_else(|| match result {
                                        Err(error) => Err(error),
                                        Ok(()) => Err(tts::TtsError::new(
                                            tts::TtsErrorKind::Protocol,
                                            "TTS Provider finished without a terminal result",
                                        )),
                                    });
                                let _ = worker_events
                                    .send(SpeechWorkerEvent::Finished(terminal))
                                    .await;
                                return;
                            }
                        }
                    }
                    let _ = with_tts_client(&tts_runtime, selected, |tts| {
                        tts.cancel_stream();
                        Ok(())
                    });
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
}
