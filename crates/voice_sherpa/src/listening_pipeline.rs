use crate::asr_stream::SherpaAsrStreamClient;
use crate::vad::SherpaVadClient;
use crate::SpeechError;
use audio::PcmAudioChunk;
use dialogue::{apply_asr_final, process_listening_pcm_chunk, DialogueSession};
use std::sync::mpsc;
use std::thread::JoinHandle;

#[cfg(test)]
#[path = "../tests/unit/listening_pipeline_tests.rs"]
mod listening_pipeline_tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ListeningPipelineEvent {
    PartialTranscript(String),
    FinalTranscript(String),
    SpeechStart,
}

pub struct SherpaListeningPipeline {
    vad: SherpaVadClient,
    asr: SherpaAsrStreamClient,
    speech_active: bool,
}

impl SherpaListeningPipeline {
    pub fn from_available_model(model: &asr::AvailableAsrModel) -> Result<Self, SpeechError> {
        Ok(Self {
            vad: SherpaVadClient::from_asr_model_dir(model.root())?,
            asr: SherpaAsrStreamClient::from_available_model(model)?,
            speech_active: false,
        })
    }

    pub fn process_chunk(
        &mut self,
        chunk: &PcmAudioChunk,
    ) -> Result<Vec<ListeningPipelineEvent>, SpeechError> {
        let outcome = process_listening_pcm_chunk(
            &mut self.vad,
            &mut self.asr,
            chunk,
            &mut self.speech_active,
        )
        .map_err(|error| SpeechError::Recognizer(error.to_string()))?;

        let mut events = Vec::new();
        for event in &outcome.vad_events {
            if *event == dialogue::VadEvent::SpeechStart {
                events.push(ListeningPipelineEvent::SpeechStart);
            }
        }
        for partial in outcome.partial_transcripts {
            events.push(ListeningPipelineEvent::PartialTranscript(partial));
        }
        if let Some(final_text) = outcome.final_transcript {
            events.push(ListeningPipelineEvent::FinalTranscript(final_text));
        }
        Ok(events)
    }
}

pub fn spawn_listening_pipeline_loop(
    model: asr::AvailableAsrModel,
    pcm_rx: async_channel::Receiver<PcmAudioChunk>,
    event_tx: async_channel::Sender<ListeningPipelineEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<JoinHandle<()>, SpeechError> {
    tracing::info!(
        target: "norma_voice",
        model_id = model.descriptor().id().as_str(),
        model_dir = %model.root().display(),
        "initializing VAD and streaming ASR pipeline"
    );
    let mut pipeline = SherpaListeningPipeline::from_available_model(&model)?;
    std::thread::Builder::new()
        .name("voice_sherpa_listening_pipeline".to_string())
        .spawn(move || {
            tracing::info!(target: "norma_voice", "VAD and streaming ASR worker running");
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match pcm_rx.try_recv() {
                    Ok(chunk) => match pipeline.process_chunk(&chunk) {
                        Ok(events) => {
                            for event in events {
                                if event_tx.try_send(event).is_err() {
                                    tracing::info!(
                                        target: "norma_voice",
                                        "VAD and streaming ASR worker stopped because event receiver closed"
                                    );
                                    return;
                                }
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                target: "norma_voice",
                                %error,
                                "listening pipeline chunk failed"
                            );
                        }
                    },
                    Err(async_channel::TryRecvError::Empty) => {
                        if stop_rx.try_recv().is_ok() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(async_channel::TryRecvError::Closed) => break,
                }
            }
            tracing::info!(target: "norma_voice", "VAD and streaming ASR worker stopped");
        })
        .map_err(|error| {
            SpeechError::Recognizer(format!(
                "could not spawn listening pipeline thread: {error}"
            ))
        })
}

pub fn apply_final_transcript_to_session(
    session: &mut DialogueSession,
    text: &str,
) -> Result<(), SpeechError> {
    apply_asr_final(session, text).map_err(|error| SpeechError::Recognizer(error.to_string()))
}
