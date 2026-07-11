use audio::PcmAudioChunk;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}

pub trait VadClient: Send {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, DialogueError>;
    fn reset(&mut self);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AsrEvent {
    Partial(String),
    Final(String),
}

pub trait AsrClient: Send {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<AsrEvent>, DialogueError>;
    fn end_utterance(&mut self) -> Result<Option<String>, DialogueError>;
    fn reset(&mut self);
}

/// Receives ordered PCM from a TTS provider running in a background worker.
pub trait TtsChunkSink: Send + Sync {
    /// Returns false when the caller no longer needs more audio for this generation.
    fn send(&self, chunk: PcmAudioChunk) -> bool;
    fn is_cancelled(&self) -> bool;
}

pub trait TtsClient: Send {
    /// Synthesizes a complete utterance for providers without native incremental output.
    fn synthesize(&mut self, text: &str) -> Result<PcmAudioChunk, DialogueError>;

    /// Streams ordered PCM to a cancellation-aware sink.
    ///
    /// Providers with native incremental output should override this method. The default keeps
    /// existing full-utterance providers compatible with the streaming playback pipeline.
    fn synthesize_stream(
        &mut self,
        text: &str,
        sink: Arc<dyn TtsChunkSink>,
    ) -> Result<(), DialogueError> {
        if sink.is_cancelled() {
            return Ok(());
        }
        let chunk = self.synthesize(text)?;
        if !sink.is_cancelled() {
            let _ = sink.send(chunk);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum DialogueError {
    #[error("{0}")]
    Message(String),
}
