//! Background runtime for Continuous Listening dialogue shell (PCM → VAD → ASR).

use super::{DialogueControlState, DialogueStartError};
use crate::settings::voice::VoiceSettingsValues;
use dialogue::DialoguePhase;
use gpui::Context;
use std::sync::mpsc;
use voice_sherpa::{
    spawn_recognizer_listening_pipeline_loop, ListeningPipelineEvent, ListeningPipelineWorker,
    PcmStreamSession, SpeechError,
};

#[cfg(test)]
#[path = "../../../tests/unit/dialogue_asr_echo_gate_tests.rs"]
mod dialogue_asr_echo_gate_tests;

#[derive(Default)]
struct AsrUtteranceGate {
    suppress_current: bool,
}

impl AsrUtteranceGate {
    fn on_speech_start(&mut self, phase: DialoguePhase) {
        self.suppress_current = phase != DialoguePhase::Listening;
    }

    fn take_final_acceptance(&mut self, phase: DialoguePhase) -> bool {
        let accepted = phase == DialoguePhase::Listening && !self.suppress_current;
        self.suppress_current = false;
        accepted
    }
}

pub(crate) struct DialogueListeningRuntime {
    pub(crate) pcm_stream: PcmStreamSession,
    pub(crate) pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
    stop_tx: Option<mpsc::Sender<()>>,
    pipeline_worker: Option<ListeningPipelineWorker>,
}

pub(crate) fn start_dialogue_listening_runtime(
    values: VoiceSettingsValues,
    model_root: std::path::PathBuf,
    input_device_id: Option<String>,
) -> Result<DialogueListeningRuntime, DialogueStartError> {
    tracing::info!(
        target: "norma_voice",
        input_device_selected = input_device_id.is_some(),
        "validating realtime voice provider"
    );
    let provider = resolve_asr_recognizer_factory(&values, &model_root).map_err(|error| {
        tracing::warn!(
            target: "norma_voice",
            stage = "provider_validation",
            %error,
            "realtime voice startup stage failed"
        );
        error
    })?;
    values
        .agent_provider_config()
        .map_err(|error| DialogueStartError::Provider(error.to_string()))?;
    tracing::info!(target: "norma_voice", "realtime voice provider validated");
    tracing::info!(target: "norma_voice", "requesting microphone authorization");
    super::check_mic_permission().map_err(|error| {
        tracing::warn!(
            target: "norma_voice",
            stage = "microphone_authorization",
            %error,
            "realtime voice startup stage failed"
        );
        DialogueStartError::Speech(error)
    })?;
    tracing::info!(target: "norma_voice", "microphone authorization accepted");
    let (pcm_stream, pcm_chunks) = voice_input::start_pcm_stream(input_device_id)
        .map_err(SpeechError::from)
        .map_err(|error| {
            tracing::warn!(
                target: "norma_voice",
                stage = "pcm_stream",
                %error,
                "realtime voice startup stage failed"
            );
            DialogueStartError::Speech(error)
        })?;
    tracing::info!(target: "norma_voice", "microphone PCM stream started");

    let (event_tx, event_rx) = async_channel::bounded::<ListeningPipelineEvent>(64);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let pipeline_worker = spawn_recognizer_listening_pipeline_loop(
        model_root, provider, pcm_chunks, event_tx, stop_rx,
    )
    .map_err(|error| {
        tracing::warn!(
            target: "norma_voice",
            stage = "vad_asr_pipeline",
            %error,
            "realtime voice startup stage failed"
        );
        DialogueStartError::Speech(error)
    })?;
    tracing::info!(target: "norma_voice", "VAD and streaming ASR pipeline started");

    Ok(DialogueListeningRuntime {
        pcm_stream,
        pipeline_events: event_rx,
        stop_tx: Some(stop_tx),
        pipeline_worker: Some(pipeline_worker),
    })
}

pub(crate) fn resolve_asr_recognizer_factory(
    values: &VoiceSettingsValues,
    model_root: &std::path::Path,
) -> Result<Box<dyn asr::RecognizerFactory>, DialogueStartError> {
    let profile = crate::settings::voice::resolve_builtin_asr_profile(values, model_root)
        .map_err(DialogueStartError::Provider)?;
    asr::builtin_recognizer_factory(profile, |credential_ref| {
        crate::settings::voice::VoiceSettingsState::load_azure_asr_secret(credential_ref)
            .map_err(asr::AsrError::Provider)
    })
    .map_err(|error| DialogueStartError::Provider(error.to_string()))
}

impl DialogueListeningRuntime {
    pub(crate) fn cancellation(&self) -> asr::RecognitionCancellation {
        self.pipeline_worker
            .as_ref()
            .expect("installed dialogue runtime must retain its ASR worker")
            .cancellation()
    }

    pub(crate) fn dispose(mut self) {
        self.stop_pipeline();
        self.pcm_stream.cancel();
    }

    fn stop_pipeline(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(worker) = self.pipeline_worker.take() {
            worker.cancel();
            let _ = worker.join();
        }
    }
}

pub(crate) fn start_pipeline_event_pump(
    dialogue: &mut DialogueControlState,
    pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
    cx: &mut Context<crate::app::MultisoulApp>,
) {
    let task = cx.spawn(async move |this, cx| {
        let mut utterance_gate = AsrUtteranceGate::default();
        while let Ok(event) = pipeline_events.recv().await {
            match event {
                ListeningPipelineEvent::PartialTranscript(partial) => {
                    tracing::debug!(
                        target: "norma_voice",
                        transcript = %partial,
                        transcript_chars = partial.chars().count(),
                        "streaming ASR partial transcript received"
                    );
                }
                ListeningPipelineEvent::SpeechStart => {
                    tracing::info!(target: "norma_voice", "VAD speech start detected");
                    let (phase, should_notify) = this
                        .update(cx, |app, _cx| {
                            let phase = app.dialogue.phase();
                            app.cancel_agent_announcements();
                            (phase, app.dialogue.on_unclassified_speech_start())
                        })
                        .unwrap_or((DialoguePhase::Idle, false));
                    utterance_gate.on_speech_start(phase);
                    if phase != DialoguePhase::Listening {
                        tracing::info!(
                            target: "norma_voice",
                            ?phase,
                            "ASR utterance marked for suppression because it began outside listening"
                        );
                    }
                    if should_notify {
                        let _ = this.update(cx, |_app, cx| cx.notify());
                    }
                }
                ListeningPipelineEvent::InputEnded(reason) => {
                    tracing::info!(
                        target: "norma_voice",
                        ?reason,
                        "continuous ASR input ended"
                    );
                }
                ListeningPipelineEvent::Failed(message) => {
                    tracing::warn!(
                        target: "norma_voice",
                        %message,
                        "continuous ASR failed"
                    );
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.mark_runtime_error(message);
                        cx.notify();
                    });
                    break;
                }
                ListeningPipelineEvent::FinalTranscript(text) => {
                    tracing::info!(
                        target: "norma_voice",
                        transcript = %text,
                        transcript_chars = text.chars().count(),
                        "streaming ASR final transcript received"
                    );
                    let phase = this
                        .update(cx, |app, _cx| app.dialogue.phase())
                        .unwrap_or(DialoguePhase::Idle);
                    let should_notify = if !utterance_gate.take_final_acceptance(phase) {
                        tracing::info!(
                            target: "norma_voice",
                            ?phase,
                            transcript = %text,
                            "ASR final transcript suppressed because utterance began during assistant activity"
                        );
                        false
                    } else {
                        this.update(cx, |app, _cx| {
                            if app.dialogue.phase() != DialoguePhase::Listening {
                                tracing::warn!(
                                    target: "norma_voice",
                                    phase = ?app.dialogue.phase(),
                                    "final transcript ignored because dialogue is not listening"
                                );
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
                        .unwrap_or(false)
                    };
                    if should_notify {
                        let _ = this.update(cx, |_app, cx| cx.notify());
                    }
                }
            }
            let still_active = this
                .update(cx, |app, _cx| app.dialogue.is_active())
                .unwrap_or(false);
            if !still_active {
                tracing::info!(
                    target: "norma_voice",
                    "stopping realtime voice event pump because dialogue is inactive"
                );
                break;
            }
        }
        tracing::info!(target: "norma_voice", "realtime voice event pump stopped");
    });
    dialogue.pcm_pump_task = Some(task);
}
