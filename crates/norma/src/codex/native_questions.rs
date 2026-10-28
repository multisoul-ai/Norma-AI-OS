//! Same-process host mailbox. The original ticket's cancellation lock guards the actual RPC.
use super::relay::{
    CodexAttachment, CodexQuestionLease, CodexQuestionRelay, CodexRelayChange, CodexRelayEvent,
};
use crate::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest, NativeQuestionReply};
use crate::protocol::{AgentProfile, NodeId, QuestionState};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum CodexNativeEvent {
    Attached(CodexAttachment),
    SessionUsed(CodexAttachment),
    Question(HookQuestion),
    Changed(CodexRelayChange),
    DeliveryFailed {
        question: CodexQuestionLease,
        reason: String,
    },
    Unavailable,
}

pub struct CodexQuestionHost {
    events: Receiver<CodexNativeEvent>,
    running: Arc<AtomicBool>,
    available: Arc<AtomicBool>,
    input_pending: Arc<AtomicBool>,
    node_id: NodeId,
    process_id: String,
}

impl CodexQuestionHost {
    pub fn start(
        relay: CodexQuestionRelay,
        node_id: NodeId,
        process_id: String,
    ) -> Result<Self, String> {
        if process_id.trim().is_empty() || process_id.len() > 128 {
            return Err("invalid_question_process".into());
        }
        let (sender, events) = bounded(128);
        let running = Arc::new(AtomicBool::new(true));
        let available = Arc::new(AtomicBool::new(true));
        let input_pending = relay.input_pending.clone();
        let mut worker = Worker {
            relay,
            sender,
            running: running.clone(),
            available: available.clone(),
            node_id,
            process_id: process_id.clone(),
            questions: Vec::new(),
        };
        std::thread::Builder::new()
            .name("codex-question-host".into())
            .spawn(move || {
                worker.pump();
                worker.disable();
                worker.running.store(false, Ordering::Release);
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            events,
            running,
            available,
            input_pending,
            node_id,
            process_id,
        })
    }
    pub fn events(&self) -> &Receiver<CodexNativeEvent> {
        &self.events
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
    pub fn process_id(&self) -> &str {
        &self.process_id
    }
    pub fn input_pending(&self) -> bool {
        self.input_pending.load(Ordering::Acquire)
    }
}

impl Drop for CodexQuestionHost {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

struct Waiting {
    lease: CodexQuestionLease,
    ticket: HookQuestion,
    sending: Option<Receiver<Result<(), String>>>,
}
struct Worker {
    relay: CodexQuestionRelay,
    sender: Sender<CodexNativeEvent>,
    running: Arc<AtomicBool>,
    available: Arc<AtomicBool>,
    node_id: NodeId,
    process_id: String,
    questions: Vec<Waiting>,
}

impl Worker {
    fn pump(&mut self) {
        while self.running.load(Ordering::Acquire) {
            for _ in 0..128 {
                let Ok(event) = self.relay.events().try_recv() else {
                    break;
                };
                self.observe(event);
            }
            if !self.relay.is_running() {
                // Running=false is stored after the producer's last send. Drain that tail too.
                while let Ok(event) = self.relay.events().try_recv() {
                    self.observe(event);
                }
                break;
            }
            if self.available.load(Ordering::Acquire) {
                if !self.relay.is_observing() {
                    self.disable();
                } else {
                    self.answer_ready();
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn observe(&mut self, event: CodexRelayEvent) {
        if !self.available.load(Ordering::Acquire)
            && matches!(&event, CodexRelayEvent::Question(change) if change.state == QuestionState::Pending)
        {
            return;
        }
        match event {
            CodexRelayEvent::SessionUsed(attachment) => {
                self.emit(CodexNativeEvent::SessionUsed(attachment))
            }
            CodexRelayEvent::Session(attachment) => {
                for waiting in self.questions.drain(..) {
                    waiting.ticket.cancel();
                }
                self.emit(CodexNativeEvent::Attached(attachment));
            }
            CodexRelayEvent::Question(change) if change.state == QuestionState::Pending => {
                if self.questions.len() >= 128 {
                    self.disable();
                    return;
                }
                let question = change.question.question();
                let ticket = HookQuestion::new(
                    HookQuestionRequest {
                        run_id: self.process_id.clone(),
                        event: AgentHookEvent {
                            control_run_id: Some(self.process_id.clone()),
                            node_id: self.node_id,
                            profile: AgentProfile::Codex,
                            provider_event: "NormaQuestion".into(),
                            session_id: Some(question.thread_id().into()),
                            turn_id: Some(question.turn_id().into()),
                            tool_name: Some("request_user_input".into()),
                            message: None,
                            raw_json: json!({"connection_id":change.question.connection_id(),
                            "request":question.original_request(),"source":question.original_source()}),
                        },
                    },
                    Instant::now(),
                );
                let Ok(ticket) = ticket else {
                    self.disable();
                    return;
                };
                self.questions.push(Waiting {
                    lease: change.question,
                    ticket: ticket.clone(),
                    sending: None,
                });
                self.emit(CodexNativeEvent::Question(ticket));
            }
            CodexRelayEvent::Question(change) => {
                if !matches!(
                    change.state,
                    QuestionState::Submitting | QuestionState::Submitted
                ) {
                    for waiting in &self.questions {
                        if waiting.lease.connection_id() == change.question.connection_id()
                            && waiting.lease.question() == change.question.question()
                        {
                            waiting.ticket.cancel();
                        }
                    }
                }
                if matches!(
                    change.state,
                    QuestionState::Answered
                        | QuestionState::AnsweredElsewhere
                        | QuestionState::Cancelled
                ) {
                    self.questions.retain(|waiting| {
                        waiting.lease.connection_id() != change.question.connection_id()
                            || waiting.lease.question() != change.question.question()
                    });
                }
                self.emit(CodexNativeEvent::Changed(change));
            }
            CodexRelayEvent::Unavailable(_) => self.disable(),
        }
    }

    fn answer_ready(&mut self) {
        let mut failed = Vec::new();
        for waiting in &mut self.questions {
            if let Some(response) = waiting.ticket.take_response(Instant::now()) {
                let result = serde_json::from_value::<NativeQuestionReply>(response)
                    .map_err(|_| "question_answer_invalid".to_owned())
                    .and_then(|reply| {
                        self.relay.answer_ticket(
                            waiting.lease.clone(),
                            reply.answers,
                            reply.delivery_id,
                            waiting.ticket.clone(),
                        )
                    });
                match result {
                    Ok(receiver) => waiting.sending = Some(receiver),
                    Err(reason) => {
                        waiting.ticket.cancel();
                        failed.push(CodexNativeEvent::DeliveryFailed {
                            question: waiting.lease.clone(),
                            reason,
                        });
                    }
                }
            }
            let Some(receiver) = &waiting.sending else {
                continue;
            };
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Disconnected) => Err("question_transport_closed".into()),
            };
            waiting.sending = None;
            if let Err(reason) = result {
                waiting.ticket.cancel();
                failed.push(CodexNativeEvent::DeliveryFailed {
                    question: waiting.lease.clone(),
                    reason,
                });
            }
        }
        for event in failed {
            self.emit(event);
        }
    }

    fn emit(&mut self, event: CodexNativeEvent) {
        if self.sender.try_send(event).is_err() {
            self.disable();
        }
    }

    fn disable(&mut self) {
        if !self.available.swap(false, Ordering::AcqRel) {
            return;
        }
        self.relay.disable_questions();
        for waiting in &self.questions {
            waiting.ticket.cancel();
        }
        let _ = self.sender.try_send(CodexNativeEvent::Unavailable);
    }
}
