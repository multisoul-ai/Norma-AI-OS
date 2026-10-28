//! Background-only discovery, once-only HTTP delivery and original message reconciliation.
use super::client::{OpenCodeQuestionClient, OpenCodeQuestionLease, ReplyError};
use super::foreground::Foreground;
mod delivery;
mod foreground;
mod receipts;
use crate::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest, NativeQuestionReply};
use crate::protocol::{AgentProfile, NodeId, QuestionState};
use crossbeam_channel::{bounded, Receiver, Sender};
use delivery::deliver;
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum OpenCodeNativeEvent {
    Question(HookQuestion),
    Changed {
        ticket_id: String,
        state: QuestionState,
        delivery_id: Option<String>,
        evidence: Option<String>,
        error: Option<String>,
    },
    Unavailable,
    ForegroundChanged {
        session: Option<String>,
        connection: String,
    },
}

pub struct OpenCodeQuestionHost {
    events: Receiver<OpenCodeNativeEvent>,
    running: Arc<AtomicBool>,
    available: Arc<AtomicBool>,
    blocked: Arc<AtomicBool>,
    tickets: Arc<Mutex<Vec<HookQuestion>>>,
    node: NodeId,
    process: String,
    session: String,
    connection: String,
    current: Arc<Mutex<Option<(String, String)>>>,
}
impl OpenCodeQuestionHost {
    pub fn start_following(
        client: OpenCodeQuestionClient,
        node: NodeId,
        process: String,
        route: std::path::PathBuf,
        nonce: String,
    ) -> Result<Self, String> {
        Self::start_inner(client, node, process, Some(Foreground::new(route, nonce)))
    }

    pub fn start(
        client: OpenCodeQuestionClient,
        node: NodeId,
        process: String,
    ) -> Result<Self, String> {
        Self::start_inner(client, node, process, None)
    }
    fn start_inner(
        client: OpenCodeQuestionClient,
        node: NodeId,
        process: String,
        foreground: Option<Foreground>,
    ) -> Result<Self, String> {
        if process.is_empty() || process.len() > 128 {
            return Err("invalid_question_process".into());
        }
        let (sender, events) = bounded(128);
        let running = Arc::new(AtomicBool::new(true));
        let available = Arc::new(AtomicBool::new(true));
        let blocked = Arc::new(AtomicBool::new(true));
        let tickets = Arc::new(Mutex::new(Vec::new()));
        let current =
            Arc::new(Mutex::new(foreground.is_none().then(|| {
                (client.session_id().into(), client.connection_id().into())
            })));
        let host = Self {
            events,
            current: current.clone(),
            running: running.clone(),
            available: available.clone(),
            blocked: blocked.clone(),
            tickets: tickets.clone(),
            node,
            process: process.clone(),
            session: client.session_id().into(),
            connection: client.connection_id().into(),
        };
        std::thread::Builder::new()
            .name("opencode-question-host".into())
            .spawn(move || {
                let mut worker = Worker {
                    client,
                    sender,
                    running,
                    available,
                    blocked,
                    tickets,
                    node,
                    process,
                    waiting: Vec::new(),
                    foreground,
                    current,
                    sent: std::collections::HashSet::new(),
                    recovering: false,
                    retired: std::collections::VecDeque::new(),
                };
                while worker.running.load(Ordering::Acquire) {
                    worker.tick();
                    for _ in 0..20 {
                        if !worker.running.load(Ordering::Acquire) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                worker.disable();
            })
            .map_err(|error| error.to_string())?;
        Ok(host)
    }
    pub fn matches_session(&self, session: &str) -> bool {
        self.current
            .lock()
            .expect("foreground poisoned")
            .as_ref()
            .is_some_and(|(s, _)| s == session)
    }
    pub fn matches_connection(&self, session: &str, connection: &str) -> bool {
        self.current
            .lock()
            .expect("foreground poisoned")
            .as_ref()
            .is_some_and(|(s, c)| s == session && c == connection)
    }
    pub fn events(&self) -> &Receiver<OpenCodeNativeEvent> {
        &self.events
    }
    pub fn node_id(&self) -> NodeId {
        self.node
    }
    pub fn process_id(&self) -> &str {
        &self.process
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn connection_id(&self) -> &str {
        &self.connection
    }
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
    pub fn input_pending(&self) -> bool {
        self.blocked.load(Ordering::Acquire)
    }
}
impl Drop for OpenCodeQuestionHost {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.available.store(false, Ordering::Release);
        for ticket in self
            .tickets
            .lock()
            .expect("question ownership poisoned")
            .iter()
        {
            ticket.cancel();
        }
    }
}

struct Waiting {
    client: OpenCodeQuestionClient,
    lease: OpenCodeQuestionLease,
    ticket: HookQuestion,
    reply: Option<NativeQuestionReply>,
    accepted: bool,
    uncertain: bool,
}
struct Worker {
    client: OpenCodeQuestionClient,
    sender: Sender<OpenCodeNativeEvent>,
    running: Arc<AtomicBool>,
    available: Arc<AtomicBool>,
    blocked: Arc<AtomicBool>,
    tickets: Arc<Mutex<Vec<HookQuestion>>>,
    node: NodeId,
    process: String,
    waiting: Vec<Waiting>,
    foreground: Option<Foreground>,
    current: Arc<Mutex<Option<(String, String)>>>,
    sent: std::collections::HashSet<(String, String)>,
    recovering: bool,
    retired: std::collections::VecDeque<Waiting>,
}
impl Worker {
    fn tick(&mut self) {
        // Even a previously idle foreground may change while any HTTP point-read blocks.
        // Only the fully reconciled snapshot below may reopen ordinary task input.
        self.blocked.store(true, Ordering::Release);
        self.reconcile_retired();
        if !self.sync_foreground() {
            return;
        }

        let Ok(pending) = self.client.pending() else {
            self.blocked.store(true, Ordering::Release);
            self.recovering = true;
            self.disable();
            return;
        };
        // The original TUI can change routes while the blocking HTTP query is in flight.
        // Its result belongs only to the foreground revision checked before that query.
        if !self.foreground_is_current() {
            self.blocked.store(true, Ordering::Release);
            self.sync_foreground();
            return;
        }
        if self.recovering {
            self.reconnect();
            return;
        }
        self.update_input_block(&pending);
        for lease in &pending {
            if self.waiting.iter().any(|waiting| {
                waiting.lease.question().request_id() == lease.question().request_id()
            }) {
                continue;
            }
            if !self.available.load(Ordering::Acquire)
                || self.sent.contains(&(
                    lease.question().session_id().into(),
                    lease.question().request_id().into(),
                ))
            {
                continue;
            }
            if self.waiting.len() + self.retired.len() >= 128 || self.sent.len() >= 4096 {
                self.disable();
                break;
            }
            let question = lease.question();
            let ticket = HookQuestion::new(
                HookQuestionRequest {
                    run_id: self.process.clone(),
                    event: AgentHookEvent {
                        control_run_id: Some(self.process.clone()),
                        node_id: self.node,
                        profile: AgentProfile::OpenCode,
                        provider_event: "NormaQuestion".into(),
                        session_id: Some(self.client.session_id().into()),
                        turn_id: None,
                        tool_name: Some("question".into()),
                        message: None,
                        raw_json: json!({"connection_id":self.client.connection_id(),"request":question.original_request()}),
                    },
                },
                Instant::now(),
            );
            let Ok(ticket) = ticket else {
                self.disable();
                break;
            };
            {
                let mut owned = self.tickets.lock().expect("question ownership poisoned");
                if !self.running.load(Ordering::Acquire) {
                    ticket.cancel();
                    return;
                }
                owned.push(ticket.clone());
            }
            self.waiting.push(Waiting {
                client: self.client.clone(),
                lease: lease.clone(),
                ticket: ticket.clone(),
                reply: None,
                accepted: false,
                uncertain: false,
            });
            self.emit(OpenCodeNativeEvent::Question(ticket));
        }
        let mut events = Vec::new();
        let mut finished = Vec::new();
        for waiting in &mut self.waiting {
            if let Some(current) = pending.iter().find(|current| {
                current.question().request_id() == waiting.lease.question().request_id()
            }) {
                if current.question() != waiting.lease.question() {
                    waiting.ticket.cancel();
                    events.push(waiting.change(
                        QuestionState::Unknown,
                        None,
                        Some("opencode_question_changed".into()),
                    ));
                    continue;
                }
                if self.available.load(Ordering::Acquire) && self.running.load(Ordering::Acquire) {
                    if let Some(event) =
                        deliver(&self.client, waiting, &self.running, &mut self.foreground)
                    {
                        events.push(event);
                    }
                }
                continue;
            }
            if let Some(event) = waiting.closed_event() {
                events.push(event);
                finished.push(waiting.ticket.id().to_owned());
            }
        }
        self.remember_sends();
        self.waiting
            .retain(|waiting| !finished.iter().any(|id| id == waiting.ticket.id()));
        self.tickets
            .lock()
            .expect("question ownership poisoned")
            .retain(|ticket| !finished.iter().any(|id| id == ticket.id()));
        // Receipt point-reads and delivery preparation can also outlive a route revision.
        self.update_input_block(&pending);
        for event in events {
            self.emit(event);
        }
    }
    fn foreground_is_current(&mut self) -> bool {
        self.foreground
            .as_mut()
            .is_none_or(Foreground::still_current)
    }
    fn update_input_block(&mut self, pending: &[OpenCodeQuestionLease]) {
        let blocked = !self.foreground_is_current()
            || !self.available.load(Ordering::Acquire)
            || !pending.is_empty()
            || !self.waiting.is_empty()
            || self.retired.iter().any(|waiting| {
                waiting.lease.question().session_id() == self.client.session_id()
                    && waiting.ticket.native_send_status()
                        != crate::agent_hook::NativeSendStatus::Unsent
            });
        self.blocked.store(blocked, Ordering::Release);
    }
    fn emit(&mut self, event: OpenCodeNativeEvent) {
        if self.sender.try_send(event).is_err() {
            self.disable();
        }
    }
    fn disable(&mut self) {
        self.blocked.store(true, Ordering::Release);
        if !self.available.swap(false, Ordering::AcqRel) {
            return;
        }
        for ticket in self
            .tickets
            .lock()
            .expect("question ownership poisoned")
            .iter()
        {
            ticket.cancel();
        }
        let _ = self.sender.try_send(OpenCodeNativeEvent::Unavailable);
    }
}
impl Waiting {
    fn change(
        &self,
        state: QuestionState,
        evidence: Option<String>,
        error: Option<String>,
    ) -> OpenCodeNativeEvent {
        OpenCodeNativeEvent::Changed {
            ticket_id: self.ticket.id().into(),
            state,
            delivery_id: self.reply.as_ref().map(|reply| reply.delivery_id.clone()),
            evidence,
            error,
        }
    }
}
