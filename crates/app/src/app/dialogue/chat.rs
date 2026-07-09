//! Background runtime for Always Listening dialogue shell (PCM → VAD → ASR).

use super::DialogueStartError;
use std::sync::mpsc;
use std::thread::JoinHandle;
use voice_sherpa::{
    spawn_listening_pipeline_loop, ListeningPipelineEvent, PcmStreamSession, SherpaProvider,
};

pub(crate) struct DialogueListeningRuntime {
    pub(crate) pcm_stream: PcmStreamSession,
    pub(crate) pipeline_events: async_channel::Receiver<ListeningPipelineEvent>,
    stop_tx: Option<mpsc::Sender<()>>,
    pipeline_thread: Option<JoinHandle<()>>,
}

pub(crate) fn start_dialogue_listening_runtime(
    provider: SherpaProvider,
    input_device_id: Option<String>,
) -> Result<DialogueListeningRuntime, DialogueStartError> {
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
