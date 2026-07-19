use audio::PcmAudioChunk;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsErrorKind {
    Configuration,
    Authentication,
    RateLimit,
    InvalidParameter,
    Transport,
    Protocol,
    AudioDecode,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct TtsError {
    kind: TtsErrorKind,
    message: String,
    retryable: bool,
    task_accepted: bool,
}

impl TtsError {
    pub fn new(kind: TtsErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable: false,
            task_accepted: false,
        }
    }

    pub fn retryable_before_acceptance(mut self) -> Self {
        self.retryable = true;
        self
    }

    pub fn after_task_accepted(mut self) -> Self {
        self.task_accepted = true;
        self.retryable = false;
        self
    }

    pub fn kind(&self) -> TtsErrorKind {
        self.kind
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn task_accepted(&self) -> bool {
        self.task_accepted
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TtsStreamSummary {
    trace_id: Option<String>,
    session_id: Option<String>,
    usage_characters: u64,
    chunk_count: u64,
    byte_count: u64,
}

impl TtsStreamSummary {
    pub fn new(
        trace_id: Option<String>,
        session_id: Option<String>,
        usage_characters: u64,
        chunk_count: u64,
        byte_count: u64,
    ) -> Self {
        Self {
            trace_id,
            session_id,
            usage_characters,
            chunk_count,
            byte_count,
        }
    }

    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn usage_characters(&self) -> u64 {
        self.usage_characters
    }

    pub fn chunk_count(&self) -> u64 {
        self.chunk_count
    }

    pub fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

/// Receives ordered PCM and exactly one terminal event from a TTS reply stream.
pub trait TtsChunkSink: Send + Sync {
    /// Returns false when the caller no longer accepts audio for this generation.
    fn send(&self, chunk: PcmAudioChunk) -> bool;
    fn finish(&self, result: Result<TtsStreamSummary, TtsError>);
    fn is_cancelled(&self) -> bool;
}

/// Provider-neutral, reply-level synthesis lifecycle.
pub trait TtsClient: Send {
    fn start_stream(&mut self, sink: Arc<dyn TtsChunkSink>) -> Result<(), TtsError>;
    fn push_text(&mut self, text: &str) -> Result<(), TtsError>;
    fn finish_stream(&mut self) -> Result<(), TtsError>;
    fn cancel_stream(&mut self);
}
