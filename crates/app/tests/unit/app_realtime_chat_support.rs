use super::{
    chat::{RealtimePcmStream, RealtimeSidecarClient},
    RealtimeControlState, REALTIME_PROCESS_TEST_LOCK,
};
use crate::app::voice::playback::AssistantAudioSink;
use realtime::{
    PcmAudioChunk, PcmFormat, RealtimeCommand, RealtimeEvent, RealtimeSessionConfig,
    RealtimeSessionId, RealtimeSidecarError,
};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn config(session_id: &str) -> RealtimeSessionConfig {
    RealtimeSessionConfig {
        session_id: RealtimeSessionId::new(session_id),
        model: "gpt-realtime-2".to_string(),
        tools: Vec::new(),
    }
}

pub(crate) fn chunk(bytes: Vec<u8>) -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, bytes)
        .expect("test PCM chunk should use valid 24k mono PCM16 metadata")
}

pub(crate) fn with_realtime_process_guard(assertions: impl FnOnce()) {
    let _guard = REALTIME_PROCESS_TEST_LOCK
        .lock()
        .expect("realtime process test lock should not be poisoned");
    RealtimeControlState::reset_process_guard_for_tests();
    assertions();
    RealtimeControlState::reset_process_guard_for_tests();
}

#[derive(Clone, Default)]
pub(crate) struct RecordingSidecar {
    inner: Arc<Mutex<RecordingSidecarState>>,
}

#[derive(Default)]
struct RecordingSidecarState {
    commands: Vec<RealtimeCommand>,
    events: VecDeque<Result<RealtimeEvent, RealtimeSidecarError>>,
    stop_calls: usize,
}

impl RecordingSidecar {
    pub(crate) fn commands(&self) -> Vec<RealtimeCommand> {
        self.inner
            .lock()
            .expect("recording sidecar lock should not be poisoned")
            .commands
            .clone()
    }

    pub(crate) fn push_event(&self, event: Result<RealtimeEvent, RealtimeSidecarError>) {
        self.inner
            .lock()
            .expect("recording sidecar lock should not be poisoned")
            .events
            .push_back(event);
    }

    pub(crate) fn stop_calls(&self) -> usize {
        self.inner
            .lock()
            .expect("recording sidecar lock should not be poisoned")
            .stop_calls
    }
}

impl RealtimeSidecarClient for RecordingSidecar {
    fn enqueue(&self, command: RealtimeCommand) -> Result<(), RealtimeSidecarError> {
        let mut inner = self
            .inner
            .lock()
            .expect("recording sidecar lock should not be poisoned");
        inner.commands.push(command.clone());
        match &command {
            RealtimeCommand::StartSession(config) => {
                inner.events.push_back(Ok(RealtimeEvent::SessionStarted {
                    session_id: config.session_id.clone(),
                }));
            }
            RealtimeCommand::StopSession { session_id } => {
                inner.events.push_back(Ok(RealtimeEvent::SessionStopped {
                    session_id: session_id.clone(),
                    reason: "user stopped".to_string(),
                }));
            }
            RealtimeCommand::Interrupt { session_id } => {
                inner
                    .events
                    .push_back(Ok(RealtimeEvent::AssistantAudioState {
                        session_id: session_id.clone(),
                        speaking: false,
                    }));
            }
            RealtimeCommand::InputAudioChunk { session_id, .. } => {
                inner
                    .events
                    .push_back(Ok(RealtimeEvent::AssistantAudioState {
                        session_id: session_id.clone(),
                        speaking: true,
                    }));
            }
            RealtimeCommand::ToolResult(_) => {}
        }
        Ok(())
    }

    fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>> {
        self.inner
            .lock()
            .expect("recording sidecar lock should not be poisoned")
            .events
            .pop_front()
    }

    fn stop(self: Box<Self>, _timeout: Duration) -> Result<(), RealtimeSidecarError> {
        self.inner
            .lock()
            .expect("recording sidecar lock should not be poisoned")
            .stop_calls += 1;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct RecordingPcmStream {
    stop_calls: Arc<AtomicUsize>,
}

impl RecordingPcmStream {
    pub(crate) fn stop_calls(&self) -> usize {
        self.stop_calls.load(Ordering::SeqCst)
    }
}

impl RealtimePcmStream for RecordingPcmStream {
    fn stop(self: Box<Self>) {
        self.stop_calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Default)]
pub(crate) struct RecordingAudioSink {
    inner: Arc<Mutex<RecordingAudioSinkState>>,
}

#[derive(Default)]
struct RecordingAudioSinkState {
    chunks: Vec<PcmAudioChunk>,
    stop_calls: usize,
}

impl RecordingAudioSink {
    pub(crate) fn chunks(&self) -> Vec<PcmAudioChunk> {
        self.inner
            .lock()
            .expect("recording audio sink lock should not be poisoned")
            .chunks
            .clone()
    }

    pub(crate) fn stop_calls(&self) -> usize {
        self.inner
            .lock()
            .expect("recording audio sink lock should not be poisoned")
            .stop_calls
    }
}

impl AssistantAudioSink for RecordingAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) {
        self.inner
            .lock()
            .expect("recording audio sink lock should not be poisoned")
            .chunks
            .push(chunk.clone());
    }

    fn stop(&mut self) {
        self.inner
            .lock()
            .expect("recording audio sink lock should not be poisoned")
            .stop_calls += 1;
    }
}

pub(crate) fn wait_until(deadline: Instant, predicate: impl Fn() -> bool, message: &str) {
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        thread::sleep(Duration::from_millis(5));
    }
}
