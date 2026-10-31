//! One native TUI and its own app-server socket. Automation failure leaves native frames flowing.
use super::questions::{CodexQuestion, CodexQuestionChange, CodexQuestions};
use crate::protocol::QuestionAnswer;
use crossbeam_channel::{bounded, Receiver, Sender};
use serde_json::Value;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tungstenite::{protocol::WebSocketConfig, Message, WebSocket};

mod answers;
mod frames;
mod input;
mod transport;
use transport::{flush, receive, send, ReadFrame};

const CAPACITY: usize = 128;
// A native configuration/context response is ~23 MiB on the tested installation.
// Keep transport at tungstenite's 64 MiB message budget; question ledgers remain 1/8 MiB.
const MAX_MESSAGE: usize = 64 * 1024 * 1024;
type Socket = WebSocket<UnixStream>;
type AnswerResult = Result<(), String>;

#[derive(Clone, Debug)]
pub enum CodexRelayEvent {
    Session(CodexAttachment),
    SessionUsed(CodexAttachment),
    Question(CodexRelayChange),
    Unavailable(String),
}

/// Issued by the owning native connection after its real start/resume response.
/// It cannot be reconstructed from question JSON.
#[derive(Clone, Debug)]
pub struct CodexAttachment {
    connection_id: String,
    thread_id: String,
    resumed: bool,
}
impl CodexAttachment {
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
    pub fn resumed(&self) -> bool {
        self.resumed
    }
}

#[derive(Clone, Debug)]
pub struct CodexRelayChange {
    pub question: CodexQuestionLease,
    pub state: crate::protocol::QuestionState,
    pub evidence: Option<String>,
    pub delivery_id: Option<String>,
}

/// Created only from the current native attachment, never reconstructed from RPC content alone.
#[derive(Clone, Debug)]
pub struct CodexQuestionLease {
    owner: String,
    generation: u64,
    question: CodexQuestion,
}

impl CodexQuestionLease {
    pub fn question(&self) -> &CodexQuestion {
        &self.question
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn connection_id(&self) -> String {
        format!("codex-relay:{}:{}", self.owner, self.generation)
    }
}

struct Answer {
    question: CodexQuestionLease,
    answers: Vec<QuestionAnswer>,
    result: Sender<AnswerResult>,
    delivery: Option<(String, crate::agent_hook::HookQuestion)>,
}

pub struct CodexQuestionRelay {
    commands: Sender<Answer>,
    events: Receiver<CodexRelayEvent>,
    running: Arc<AtomicBool>,
    observing: Arc<AtomicBool>,
    owner: String,
    pub(super) input_pending: Arc<AtomicBool>,
    forwarding: Arc<AtomicBool>,
}

impl CodexQuestionRelay {
    /// Handshakes perform IO. The caller must already be on a background thread.
    pub fn connect(native: UnixStream, server: &Path) -> Result<Self, String> {
        Self::connect_inner(native, server, false, true, false)
    }

    /// Production attachment: require the tested server version on the native initialize reply.
    /// `full_access` carries the original YOLO launch on session requests, because a remote TUI
    /// with permission overrides rejects /resume and fork.
    pub fn connect_verified(
        native: UnixStream,
        server: &Path,
        full_access: bool,
    ) -> Result<Self, String> {
        Self::connect_inner(native, server, true, true, full_access)
    }

    /// Complete transport handshakes, but let the host register input ownership before frames flow.
    pub fn connect_verified_paused(
        native: UnixStream,
        server: &Path,
        full_access: bool,
    ) -> Result<Self, String> {
        Self::connect_inner(native, server, true, false, full_access)
    }

    pub(super) fn start_forwarding(&self) {
        self.forwarding.store(true, Ordering::Release);
    }

    fn connect_inner(
        native: UnixStream,
        server: &Path,
        check_version: bool,
        forwarding: bool,
        full_access: bool,
    ) -> Result<Self, String> {
        let owner = crate::agent_hook::fresh_control_id().map_err(|error| error.to_string())?;
        let upstream = UnixStream::connect(server).map_err(|error| error.to_string())?;
        for stream in [&native, &upstream] {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
        }
        let config = || {
            WebSocketConfig::default()
                .max_message_size(Some(MAX_MESSAGE))
                .max_frame_size(Some(MAX_MESSAGE))
                .write_buffer_size(0)
                .max_write_buffer_size(MAX_MESSAGE * 2)
        };
        let (upstream, _) =
            tungstenite::client::client_with_config("ws://localhost/", upstream, Some(config()))
                .map_err(|error| error.to_string())?;
        let downstream = tungstenite::accept_with_config(native, Some(config()))
            .map_err(|error| error.to_string())?;
        for socket in [&upstream, &downstream] {
            socket
                .get_ref()
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
        }
        let (commands, receiver) = bounded(CAPACITY);
        let (sender, events) = bounded(CAPACITY);
        let running = Arc::new(AtomicBool::new(true));
        let observing = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let worker_observing = observing.clone();
        let worker_owner = owner.clone();
        let inputs = input::NativeInputs::default();
        let input_pending = inputs.blocked.clone();
        let forwarding = Arc::new(AtomicBool::new(forwarding));
        let worker_forwarding = forwarding.clone();
        std::thread::Builder::new()
            .name("codex-native-relay".into())
            .spawn(move || {
                let mut relay = Relay {
                    native: downstream,
                    server: upstream,
                    commands: receiver,
                    events: sender,
                    running: worker_running,
                    observing: worker_observing,
                    ledger: None,
                    starts: Vec::new(),
                    generation: 0,
                    owner: worker_owner,
                    pending_send: None,
                    disabled: false,
                    inputs,
                    initialize: None,
                    version_verified: !check_version,
                    full_access,
                };
                while relay.running.load(Ordering::Acquire)
                    && !worker_forwarding.load(Ordering::Acquire)
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                let result = relay.pump();
                if let Err(reason) = &result {
                    tracing::warn!(%reason, "native Codex relay disconnected");
                }
                relay.disable(
                    result
                        .as_ref()
                        .err()
                        .map(String::as_str)
                        .unwrap_or("codex_native_connection_closed"),
                );
                relay.running.store(false, Ordering::Release);
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            commands,
            events,
            running,
            observing,
            owner,
            input_pending,
            forwarding,
        })
    }

    pub fn events(&self) -> &Receiver<CodexRelayEvent> {
        &self.events
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    pub fn is_observing(&self) -> bool {
        self.observing.load(Ordering::Acquire)
    }

    /// Enqueue once. An accepted or written frame is not evidence of tool consumption.
    pub fn answer(
        &self,
        question: CodexQuestionLease,
        answers: Vec<QuestionAnswer>,
    ) -> Result<Receiver<AnswerResult>, String> {
        if !self.is_running() || !self.is_observing() {
            return Err("codex_question_relay_unavailable".into());
        }
        if question.owner != self.owner {
            return Err("codex_question_connection_changed".into());
        }
        question
            .question
            .response(&answers)
            .map_err(str::to_owned)?;
        let (result, receiver) = bounded(1);
        self.commands
            .try_send(Answer {
                question,
                answers,
                result,
                delivery: None,
            })
            .map_err(|_| "codex_question_queue_full")?;
        Ok(receiver)
    }

    /// Disable only remote answering. The original native client keeps the same connection.
    pub fn disable_questions(&self) {
        self.observing.store(false, Ordering::Release);
    }
}

impl Drop for CodexQuestionRelay {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

struct Relay {
    native: Socket,
    server: Socket,
    commands: Receiver<Answer>,
    events: Sender<CodexRelayEvent>,
    running: Arc<AtomicBool>,
    observing: Arc<AtomicBool>,
    ledger: Option<CodexQuestions>,
    starts: Vec<(Value, Option<String>)>,
    pending_send: Option<Answer>,
    generation: u64,
    owner: String,
    disabled: bool,
    inputs: input::NativeInputs,
    initialize: Option<Value>,
    version_verified: bool,
    full_access: bool,
}

impl Relay {
    fn pump(&mut self) -> Result<(), String> {
        while self.running.load(Ordering::Acquire) {
            let mut active = false;
            // A WouldBlock send has buffered its frame. Flush before reading more into that side.
            let mut native_ready = flush(&mut self.native)?;
            let mut server_ready = self.flush_answer()?;
            let mut native_drained = false;
            let mut server_drained = false;
            // Drain native decisions and server resolutions before accepting a remote answer.
            for _ in 0..32 {
                if !server_ready {
                    break;
                }
                let frame = match receive(&mut self.native)? {
                    ReadFrame::Data(frame) => frame,
                    ReadFrame::Control => {
                        active = true;
                        continue;
                    }
                    ReadFrame::Pending => {
                        native_drained = true;
                        break;
                    }
                };
                active = true;
                let frame = self.native_frame(frame);
                server_ready = send(&mut self.server, frame)?;
            }
            for _ in 0..32 {
                // tungstenite read() may flush queued writes while replying to Ping.
                // An answer must be flushed under its ticket lock before this path can read.
                if !native_ready || self.pending_send.is_some() {
                    break;
                }
                let frame = match receive(&mut self.server)? {
                    ReadFrame::Data(frame) => frame,
                    ReadFrame::Control => {
                        active = true;
                        continue;
                    }
                    ReadFrame::Pending => {
                        server_drained = true;
                        break;
                    }
                };
                active = true;
                self.server_frame(&frame);
                native_ready = send(&mut self.native, frame)?;
            }
            if !self.observing.load(Ordering::Acquire) && !self.disabled {
                self.disable("codex_question_observer_disabled");
            }
            for _ in 0..32 {
                // A bounded batch may leave a native answer or resolved event already queued.
                // Automation waits until both sides have actually reached WouldBlock.
                if !server_ready || !native_drained || !server_drained {
                    break;
                }
                let Ok(command) = self.commands.try_recv() else {
                    break;
                };
                active = true;
                server_ready = self.send_answer(command)?;
                self.publish_changes();
            }
            if !active {
                transport::wait_ready(
                    &self.native,
                    &self.server,
                    native_ready,
                    server_ready,
                    self.pending_send.is_none(),
                )?;
            }
        }
        Ok(())
    }

    fn publish_changes(&mut self) {
        let changes = self
            .ledger
            .as_mut()
            .map(CodexQuestions::take_changes)
            .unwrap_or_default();
        for change in changes {
            if self.events.try_send(self.change_event(change)).is_err() {
                self.disable("codex_question_event_gap");
                break;
            }
        }
    }

    fn disable(&mut self, reason: &str) {
        self.close_answers(reason);
        self.observing.store(false, Ordering::Release);
        if self.disabled {
            return;
        }
        tracing::warn!(
            reason,
            "native Codex question observation disabled; terminal passthrough retained"
        );
        self.disabled = true;
        if let Some(ledger) = &mut self.ledger {
            ledger.disconnect();
            let changes = ledger.take_changes();
            for change in changes {
                let _ = self.events.try_send(self.change_event(change));
            }
        }
        self.starts.clear();
        let _ = self
            .events
            .try_send(CodexRelayEvent::Unavailable(reason.into()));
    }

    fn change_event(&self, change: CodexQuestionChange) -> CodexRelayEvent {
        CodexRelayEvent::Question(CodexRelayChange {
            question: CodexQuestionLease {
                owner: self.owner.clone(),
                generation: self.generation,
                question: change.question,
            },
            state: change.state,
            evidence: change.evidence,
            delivery_id: change.delivery_id,
        })
    }
}
