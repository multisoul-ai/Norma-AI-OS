use super::{DialogueTtsClient, FakeTtsClient, SherpaTtsClient};
use audio::PcmAudioChunk;
use std::sync::Arc;

pub(super) struct LocalStreamState {
    sink: Arc<dyn tts::TtsChunkSink>,
    chunk_count: u64,
    byte_count: u64,
}

impl tts::TtsClient for FakeTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        start_local_stream(&mut self.stream, sink)
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        let chunk = match DialogueTtsClient::synthesize(self, text).map_err(local_synthesis_error) {
            Ok(chunk) => chunk,
            Err(error) => return fail_local_stream(&mut self.stream, error),
        };
        deliver_local_chunk(&mut self.stream, chunk)
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        finish_local_stream(&mut self.stream)
    }

    fn cancel_stream(&mut self) {
        cancel_local_stream(&mut self.stream);
    }
}

impl tts::TtsClient for SherpaTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        start_local_stream(&mut self.stream, sink)
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        let chunk = match DialogueTtsClient::synthesize(self, text).map_err(local_synthesis_error) {
            Ok(chunk) => chunk,
            Err(error) => return fail_local_stream(&mut self.stream, error),
        };
        deliver_local_chunk(&mut self.stream, chunk)
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        finish_local_stream(&mut self.stream)
    }

    fn cancel_stream(&mut self) {
        cancel_local_stream(&mut self.stream);
    }
}

fn start_local_stream(
    stream: &mut Option<LocalStreamState>,
    sink: Arc<dyn tts::TtsChunkSink>,
) -> Result<(), tts::TtsError> {
    if stream.is_some() {
        return Err(tts::TtsError::new(
            tts::TtsErrorKind::Configuration,
            "a local TTS stream is already active",
        ));
    }
    *stream = Some(LocalStreamState {
        sink,
        chunk_count: 0,
        byte_count: 0,
    });
    Ok(())
}

fn deliver_local_chunk(
    stream: &mut Option<LocalStreamState>,
    chunk: PcmAudioChunk,
) -> Result<(), tts::TtsError> {
    let active = stream.as_mut().ok_or_else(|| {
        tts::TtsError::new(
            tts::TtsErrorKind::Configuration,
            "no local TTS stream is active",
        )
    })?;
    if active.sink.is_cancelled() || !active.sink.send(chunk.clone()) {
        cancel_local_stream(stream);
        return Err(tts::TtsError::new(
            tts::TtsErrorKind::Cancelled,
            "TTS stream was cancelled",
        ));
    }
    active.chunk_count = active.chunk_count.saturating_add(1);
    active.byte_count = active.byte_count.saturating_add(chunk.bytes.len() as u64);
    Ok(())
}

fn finish_local_stream(stream: &mut Option<LocalStreamState>) -> Result<(), tts::TtsError> {
    let active = stream.take().ok_or_else(|| {
        tts::TtsError::new(
            tts::TtsErrorKind::Configuration,
            "no local TTS stream is active",
        )
    })?;
    active.sink.finish(Ok(tts::TtsStreamSummary::new(
        None,
        None,
        0,
        active.chunk_count,
        active.byte_count,
    )));
    Ok(())
}

fn cancel_local_stream(stream: &mut Option<LocalStreamState>) {
    if let Some(active) = stream.take() {
        active.sink.finish(Err(tts::TtsError::new(
            tts::TtsErrorKind::Cancelled,
            "TTS stream was cancelled",
        )));
    }
}

fn fail_local_stream(
    stream: &mut Option<LocalStreamState>,
    error: tts::TtsError,
) -> Result<(), tts::TtsError> {
    if let Some(active) = stream.take() {
        active.sink.finish(Err(error.clone()));
    }
    Err(error)
}

fn local_synthesis_error(error: dialogue::DialogueError) -> tts::TtsError {
    tts::TtsError::new(tts::TtsErrorKind::Protocol, error.to_string())
}
