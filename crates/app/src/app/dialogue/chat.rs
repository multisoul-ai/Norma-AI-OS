//! Background runtime for Always Listening dialogue shell (PCM capture only in Task 4).

use super::DialogueStartError;
use voice_sherpa::{PcmStreamSession, SherpaProvider};

pub(crate) struct DialogueListeningRuntime {
    pub(crate) pcm_stream: PcmStreamSession,
    pub(crate) pcm_chunks: async_channel::Receiver<audio::PcmAudioChunk>,
}

pub(crate) fn start_dialogue_listening_runtime(
    provider: SherpaProvider,
    input_device_id: Option<String>,
) -> Result<DialogueListeningRuntime, DialogueStartError> {
    super::check_mic_permission().map_err(DialogueStartError::Speech)?;
    let (pcm_stream, pcm_chunks) = provider
        .start_pcm_stream(input_device_id.as_deref())
        .map_err(DialogueStartError::Speech)?;
    Ok(DialogueListeningRuntime {
        pcm_stream,
        pcm_chunks,
    })
}

impl DialogueListeningRuntime {
    pub(crate) fn dispose(self) {
        self.pcm_stream.cancel();
    }
}
