use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio_util::sync::CancellationToken;

const PCM16_MONO_SAMPLE_BYTES: usize = 2;
// 20 ms capture chunks are typical: 64 slots × 20 ms = 1.28 s of bounded backlog.
const RECOGNITION_COMMAND_CAPACITY: usize = 64;
// A session emits substantially fewer events than PCM commands; keep the same bounded budget.
const RECOGNITION_EVENT_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsrProviderCapabilities {
    partial_results: bool,
    max_utterance_seconds: Option<u64>,
}

impl AsrProviderCapabilities {
    pub const fn new(partial_results: bool, max_utterance_seconds: Option<u64>) -> Self {
        Self {
            partial_results,
            max_utterance_seconds,
        }
    }

    pub const fn partial_results(self) -> bool {
        self.partial_results
    }

    pub const fn max_utterance_seconds(self) -> Option<u64> {
        self.max_utterance_seconds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEndReason {
    User,
    Vad,
    DurationLimit,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AsrError {
    #[error("recognition session is closed")]
    SessionClosed,
    #[error("recognition worker stopped unexpectedly")]
    WorkerStopped,
    #[error("invalid PCM: expected 16 kHz mono PCM16")]
    InvalidPcm,
    #[error("ASR provider failed: {0}")]
    Provider(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecognitionEvent {
    Partial(String),
    InputEnded(InputEndReason),
    Final(String),
    Empty,
    Failed(AsrError),
    Cancelled,
}

pub trait Recognizer: Send + 'static {
    fn accept_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError>;
    fn finish(&mut self) -> Result<Option<String>, AsrError>;

    fn set_cancellation_token(&mut self, _cancellation: RecognitionCancellation) {}
}

impl Recognizer for Box<dyn Recognizer> {
    fn accept_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        self.as_mut().accept_pcm(chunk)
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        self.as_mut().finish()
    }

    fn set_cancellation_token(&mut self, cancellation: RecognitionCancellation) {
        self.as_mut().set_cancellation_token(cancellation);
    }
}

pub trait RecognizerFactory: Send + Sync + 'static {
    fn capabilities(&self) -> AsrProviderCapabilities;
    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError>;
    fn create_with_cancellation(
        &self,
        cancellation: RecognitionCancellation,
    ) -> Result<Box<dyn Recognizer>, AsrError> {
        if cancellation.is_cancelled() {
            return Err(AsrError::SessionClosed);
        }
        let mut recognizer = self.create()?;
        recognizer.set_cancellation_token(cancellation.clone());
        if cancellation.is_cancelled() {
            return Err(AsrError::SessionClosed);
        }
        Ok(recognizer)
    }
}

#[derive(Clone, Debug)]
pub struct RecognitionCancellation {
    token: CancellationToken,
}

impl Default for RecognitionCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl RecognitionCancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn child(&self) -> Self {
        Self {
            token: self.token.child_token(),
        }
    }

    pub(crate) async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

enum RecognitionCommand {
    Pcm(PcmAudioChunk),
    Finish(InputEndReason),
    Cancel,
}

pub struct RecognitionPipeline {
    capabilities: AsrProviderCapabilities,
}

impl RecognitionPipeline {
    pub const fn new(capabilities: AsrProviderCapabilities) -> Self {
        Self { capabilities }
    }

    pub fn start<R: Recognizer>(&self, recognizer: R) -> RecognitionSession {
        self.start_with_cancellation(recognizer, RecognitionCancellation::new())
    }

    pub fn start_with_cancellation<R: Recognizer>(
        &self,
        mut recognizer: R,
        cancellation: RecognitionCancellation,
    ) -> RecognitionSession {
        recognizer.set_cancellation_token(cancellation.clone());
        let (command_tx, command_rx) = async_channel::bounded(RECOGNITION_COMMAND_CAPACITY);
        let (event_tx, event_rx) = async_channel::bounded(RECOGNITION_EVENT_CAPACITY);
        let worker_event_tx = event_tx.clone();
        let spawn_result = std::thread::Builder::new()
            .name("asr_recognition_pipeline".to_string())
            .spawn({
                let cancellation = cancellation.clone();
                move || {
                    run_recognition_worker(recognizer, command_rx, worker_event_tx, cancellation)
                }
            });
        if spawn_result.is_err() {
            let _ = event_tx.try_send(RecognitionEvent::Failed(AsrError::WorkerStopped));
        }
        drop(event_tx);
        RecognitionSession {
            command_tx,
            event_rx,
            accepted_pcm_bytes: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            max_pcm_bytes: self.capabilities.max_utterance_seconds.map(|seconds| {
                16_000usize
                    .saturating_mul(PCM16_MONO_SAMPLE_BYTES)
                    .saturating_mul(seconds as usize)
            }),
            cancellation,
        }
    }
}

pub struct RecognitionSession {
    command_tx: async_channel::Sender<RecognitionCommand>,
    event_rx: async_channel::Receiver<RecognitionEvent>,
    accepted_pcm_bytes: AtomicUsize,
    closed: AtomicBool,
    max_pcm_bytes: Option<usize>,
    cancellation: RecognitionCancellation,
}

impl RecognitionSession {
    pub fn reached_duration_limit(&self) -> bool {
        self.max_pcm_bytes.is_some_and(|maximum| {
            self.closed.load(Ordering::Acquire)
                && self.accepted_pcm_bytes.load(Ordering::Acquire) >= maximum
        })
    }

    pub fn push_pcm(&self, mut chunk: PcmAudioChunk) -> Result<(), AsrError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(AsrError::SessionClosed);
        }
        if chunk.sample_rate_hz != 16_000
            || chunk.channels != 1
            || !chunk.bytes.len().is_multiple_of(PCM16_MONO_SAMPLE_BYTES)
        {
            return Err(AsrError::InvalidPcm);
        }

        let accepted = self.accepted_pcm_bytes.load(Ordering::Acquire);
        let reaches_limit = self
            .max_pcm_bytes
            .is_some_and(|maximum| accepted.saturating_add(chunk.bytes.len()) >= maximum);
        if let Some(maximum) = self.max_pcm_bytes {
            let remaining = maximum.saturating_sub(accepted);
            if chunk.bytes.len() > remaining {
                chunk.bytes.truncate(remaining);
            }
        }
        self.accepted_pcm_bytes
            .fetch_add(chunk.bytes.len(), Ordering::AcqRel);
        if !chunk.bytes.is_empty() {
            self.command_tx
                .send_blocking(RecognitionCommand::Pcm(chunk))
                .map_err(|_| AsrError::SessionClosed)?;
        }
        if reaches_limit {
            self.close_with(RecognitionCommand::Finish(InputEndReason::DurationLimit))?;
        }
        Ok(())
    }

    pub fn finish(&self, reason: InputEndReason) -> Result<(), AsrError> {
        self.close_with(RecognitionCommand::Finish(reason))
    }

    pub fn cancel(&self) -> Result<(), AsrError> {
        self.cancellation.cancel();
        self.close_with(RecognitionCommand::Cancel)
    }

    pub fn cancellation(&self) -> RecognitionCancellation {
        self.cancellation.clone()
    }

    pub fn events(&self) -> async_channel::Receiver<RecognitionEvent> {
        self.event_rx.clone()
    }

    pub fn collect_events(&self) -> Vec<RecognitionEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.recv_blocking() {
            events.push(event);
        }
        events
    }

    fn close_with(&self, command: RecognitionCommand) -> Result<(), AsrError> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Err(AsrError::SessionClosed);
        }
        self.command_tx
            .send_blocking(command)
            .map_err(|_| AsrError::SessionClosed)
    }
}

impl Drop for RecognitionSession {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.command_tx.try_send(RecognitionCommand::Cancel);
        }
    }
}

fn run_recognition_worker<R: Recognizer>(
    mut recognizer: R,
    command_rx: async_channel::Receiver<RecognitionCommand>,
    event_tx: async_channel::Sender<RecognitionEvent>,
    cancellation: RecognitionCancellation,
) {
    while let Ok(command) = command_rx.recv_blocking() {
        match command {
            RecognitionCommand::Pcm(chunk) => match recognizer.accept_pcm(&chunk) {
                Ok(partials) => {
                    for partial in partials {
                        if !partial.trim().is_empty()
                            && event_tx
                                .send_blocking(RecognitionEvent::Partial(partial))
                                .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = event_tx.send_blocking(RecognitionEvent::Failed(error));
                    return;
                }
            },
            RecognitionCommand::Finish(reason) => {
                if event_tx
                    .send_blocking(RecognitionEvent::InputEnded(reason))
                    .is_err()
                {
                    return;
                }
                let terminal = match recognizer.finish() {
                    _ if cancellation.is_cancelled() => RecognitionEvent::Cancelled,
                    Ok(Some(text)) if !text.trim().is_empty() => {
                        RecognitionEvent::Final(text.trim().to_string())
                    }
                    Ok(_) => RecognitionEvent::Empty,
                    Err(error) => RecognitionEvent::Failed(error),
                };
                let _ = event_tx.send_blocking(terminal);
                return;
            }
            RecognitionCommand::Cancel => {
                let _ = event_tx.send_blocking(RecognitionEvent::Cancelled);
                return;
            }
        }
    }
    let _ = event_tx.send_blocking(RecognitionEvent::Failed(AsrError::WorkerStopped));
}
