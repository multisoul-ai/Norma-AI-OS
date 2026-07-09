use audio::PcmAudioChunk;

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

pub trait TtsClient: Send {
    /// MVP: synthesize a full utterance. Streaming can be added later without changing callers much.
    fn synthesize(&mut self, text: &str) -> Result<PcmAudioChunk, DialogueError>;
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum DialogueError {
    #[error("{0}")]
    Message(String),
}
