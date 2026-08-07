use crate::asr_stream::PipelineAsrStreamClient;
use crate::SpeechError;
use audio::PcmAudioChunk;
use dialogue::{apply_asr_final, process_listening_pcm_chunk, DialogueSession};
use std::sync::mpsc;
use std::thread::JoinHandle;
use voice_input::SherpaVadClient;

#[cfg(test)]
#[path = "../tests/unit/listening_pipeline_tests.rs"]
mod listening_pipeline_tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ListeningPipelineEvent {
    PartialTranscript(String),
    FinalTranscript(String),
    SpeechStart,
    InputEnded(dialogue::AsrInputEndReason),
    Failed(String),
}

pub struct SherpaListeningPipeline {
    vad: SherpaVadClient,
    asr: PipelineAsrStreamClient,
    speech_active: bool,
}

impl SherpaListeningPipeline {
    pub fn from_available_model(model: &asr::AvailableAsrModel) -> Result<Self, SpeechError> {
        Ok(Self {
            vad: SherpaVadClient::from_asr_model_dir(model.root())?,
            asr: PipelineAsrStreamClient::new(Box::new(asr::SherpaRecognizerFactory::new(
                model.clone(),
            )))
            .map_err(|error| SpeechError::Recognizer(error.to_string()))?,
            speech_active: false,
        })
    }

    pub fn from_recognizer_factory(
        model_root: &std::path::Path,
        factory: Box<dyn asr::RecognizerFactory>,
    ) -> Result<Self, SpeechError> {
        #[cfg(feature = "test-support")]
        let vad = {
            let _ = model_root;
            SherpaVadClient::deterministic_for_tests()
        };
        #[cfg(not(feature = "test-support"))]
        let vad = SherpaVadClient::from_asr_model_dir(&model_root.join("_cloud_asr"))?;
        Ok(Self {
            // VAD is a shared sibling under model_root/silero_vad; the provider-independent
            // resolver only needs a child path to find that shared artifact.
            vad,
            asr: PipelineAsrStreamClient::new(factory)
                .map_err(|error| SpeechError::Recognizer(error.to_string()))?,
            speech_active: false,
        })
    }

    pub fn cancellation(&self) -> asr::RecognitionCancellation {
        self.asr.cancellation()
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
        if let Some(reason) = outcome.input_ended {
            events.push(ListeningPipelineEvent::InputEnded(reason));
        }
        if let Some(final_text) = outcome.final_transcript {
            events.push(ListeningPipelineEvent::FinalTranscript(final_text));
        }
        Ok(events)
    }
}

pub struct ListeningPipelineWorker {
    cancellation: asr::RecognitionCancellation,
    thread: JoinHandle<()>,
}

impl ListeningPipelineWorker {
    pub fn cancellation(&self) -> asr::RecognitionCancellation {
        self.cancellation.clone()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn join(self) -> std::thread::Result<()> {
        self.thread.join()
    }
}

pub fn spawn_listening_pipeline_loop(
    model: asr::AvailableAsrModel,
    pcm_rx: async_channel::Receiver<PcmAudioChunk>,
    event_tx: async_channel::Sender<ListeningPipelineEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<JoinHandle<()>, SpeechError> {
    let pipeline = SherpaListeningPipeline::from_available_model(&model)?;
    spawn_initialized_pipeline_loop(pipeline, pcm_rx, event_tx, stop_rx).map(|worker| worker.thread)
}

pub fn spawn_recognizer_listening_pipeline_loop(
    model_root: std::path::PathBuf,
    factory: Box<dyn asr::RecognizerFactory>,
    pcm_rx: async_channel::Receiver<PcmAudioChunk>,
    event_tx: async_channel::Sender<ListeningPipelineEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<ListeningPipelineWorker, SpeechError> {
    tracing::info!(
        target: "norma_voice",
        model_root = %model_root.display(),
        "initializing shared VAD and provider-neutral ASR pipeline"
    );
    let pipeline = SherpaListeningPipeline::from_recognizer_factory(&model_root, factory)?;
    spawn_initialized_pipeline_loop(pipeline, pcm_rx, event_tx, stop_rx)
}

fn spawn_initialized_pipeline_loop(
    mut pipeline: SherpaListeningPipeline,
    pcm_rx: async_channel::Receiver<PcmAudioChunk>,
    event_tx: async_channel::Sender<ListeningPipelineEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<ListeningPipelineWorker, SpeechError> {
    let cancellation = pipeline.cancellation();
    let thread = std::thread::Builder::new()
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
                            let _ = event_tx.try_send(ListeningPipelineEvent::Failed(
                                error.to_string(),
                            ));
                            return;
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
        })?;
    Ok(ListeningPipelineWorker {
        cancellation,
        thread,
    })
}

pub fn apply_final_transcript_to_session(
    session: &mut DialogueSession,
    text: &str,
) -> Result<(), SpeechError> {
    apply_asr_final(session, text).map_err(|error| SpeechError::Recognizer(error.to_string()))
}
