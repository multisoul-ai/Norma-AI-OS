//! Background runtime for Always Listening dialogue shell (PCM → VAD → ASR).

use super::{DialogueControlState, DialogueStartError};
use crate::settings::voice::VoiceSettingsValues;
use dialogue::DialoguePhase;
use gpui::Context;
use std::sync::mpsc;
use std::thread::JoinHandle;
use voice_sherpa::{spawn_listening_pipeline_loop, ListeningPipelineEvent, PcmStreamSession};

pub(crate) struct DialogueListeningRuntime {
    pub(crate) pcm_stream: PcmStreamSession,
    pub(crate) pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
    stop_tx: Option<mpsc::Sender<()>>,
    pipeline_thread: Option<JoinHandle<()>>,
}

pub(crate) fn start_dialogue_listening_runtime(
    values: VoiceSettingsValues,
    input_device_id: Option<String>,
) -> Result<DialogueListeningRuntime, DialogueStartError> {
    let provider = DialogueControlState::validate_dialogue_start(&values)?;
    super::check_mic_permission().map_err(DialogueStartError::Speech)?;
    let (pcm_stream, pcm_chunks) = provider
        .start_pcm_stream(input_device_id.as_deref())
        .map_err(DialogueStartError::Speech)?;

    let (event_tx, event_rx) = async_channel::bounded::<ListeningPipelineEvent>(64);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let model_dir = provider.model_dir().to_path_buf();
    let pipeline_thread = spawn_listening_pipeline_loop(model_dir, pcm_chunks, event_tx, stop_rx)
        .map_err(DialogueStartError::Speech)?;

    Ok(DialogueListeningRuntime {
        pcm_stream,
        pipeline_events: event_rx,
        stop_tx: Some(stop_tx),
        pipeline_thread: Some(pipeline_thread),
    })
}

impl DialogueListeningRuntime {
    pub(crate) fn dispose(mut self) {
        self.stop_pipeline();
        self.pcm_stream.cancel();
    }

    fn stop_pipeline(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(thread) = self.pipeline_thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn start_pipeline_event_pump(
    dialogue: &mut DialogueControlState,
    pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
    cx: &mut Context<crate::app::MultisoulApp>,
) {
    let task = cx.spawn(async move |this, cx| {
        while let Ok(event) = pipeline_events.recv().await {
            match event {
                ListeningPipelineEvent::PartialTranscript(_partial) => {}
                ListeningPipelineEvent::SpeechStart => {
                    let should_notify = this
                        .update(cx, |app, _cx| app.dialogue.on_unclassified_speech_start())
                        .unwrap_or(false);
                    if should_notify {
                        let _ = this.update(cx, |_app, cx| cx.notify());
                    }
                }
                ListeningPipelineEvent::FinalTranscript(text) => {
                    tracing::info!(
                        transcript = %text,
                        transcript_chars = text.chars().count(),
                        "speech recognition final transcript"
                    );
                    let should_notify = this
                        .update(cx, |app, _cx| {
                            if app.dialogue.phase() != DialoguePhase::Listening {
                                return false;
                            }
                            match app.dialogue.on_final_transcript(&text) {
                                Ok(()) => {
                                    app.dialogue.mark_voice_turn_pending();
                                    true
                                }
                                Err(error) => {
                                    tracing::warn!(%error, "failed to apply final transcript");
                                    false
                                }
                            }
                        })
                        .unwrap_or(false);
                    if should_notify {
                        let _ = this.update(cx, |_app, cx| cx.notify());
                    }
                }
            }
            let still_active = this
                .update(cx, |app, _cx| app.dialogue.is_active())
                .unwrap_or(false);
            if !still_active {
                break;
            }
        }
    });
    dialogue.pcm_pump_task = Some(task);
}
