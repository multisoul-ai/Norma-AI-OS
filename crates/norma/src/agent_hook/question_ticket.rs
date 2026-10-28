//! A revocable native question Hook; taking a response is not consumption evidence.
use super::{fresh_control_id, AgentHookEvent, NativeQuestion};
use crate::protocol::QuestionAnswer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod guard;
pub use guard::QuestionSendGuard;

pub const QUESTION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookQuestionRequest {
    pub run_id: String,
    pub event: AgentHookEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestionHookStatus {
    Pending,
    Ready,
    Responding,
    Responded,
    Cancelled,
}

/// Native RPC transmission is distinct from returning an answer to a bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSendStatus {
    Unsent,
    Writing,
    Sent,
    Unknown,
}

#[derive(Debug)]
struct State {
    status: QuestionHookStatus,
    output: Option<Value>,
    delivery_id: Option<String>,
    answers: Option<Vec<QuestionAnswer>>,
    native_send: NativeSendStatus,
    send_rejection: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct HookQuestion(Arc<Invocation>);

/// Once the host commits to HTTP IO, cancellation reports in-flight instead of a false success.
/// No MutexGuard crosses the network call. Dropping without an acknowledgement is Unknown.
pub struct NativeHttpDelivery {
    ticket: HookQuestion,
    finished: bool,
}
impl NativeHttpDelivery {
    pub fn accepted(mut self) {
        self.ticket
            .0
            .state
            .lock()
            .expect("question state poisoned")
            .native_send = NativeSendStatus::Sent;
        self.finished = true;
    }
    pub fn rejected(mut self) {
        let mut state = self.ticket.0.state.lock().expect("question state poisoned");
        state.native_send = NativeSendStatus::Unsent;
        state.status = QuestionHookStatus::Cancelled;
        self.finished = true;
    }
}
impl Drop for NativeHttpDelivery {
    fn drop(&mut self) {
        if !self.finished {
            self.ticket
                .0
                .state
                .lock()
                .expect("question state poisoned")
                .native_send = NativeSendStatus::Unknown;
        }
    }
}

#[derive(Debug)]
struct Invocation {
    id: String,
    request: HookQuestionRequest,
    adapter: NativeQuestion,
    expires_at: Instant,
    state: Mutex<State>,
    guard: Mutex<guard::SendGuard>,
}

impl HookQuestion {
    pub fn new(request: HookQuestionRequest, now: Instant) -> io::Result<Self> {
        let invalid = |reason| io::Error::new(io::ErrorKind::InvalidInput, reason);
        let adapter = NativeQuestion::from_event(&request.event).map_err(invalid)?;
        if request.run_id.trim().is_empty()
            || request.run_id.len() > 128
            || request
                .event
                .control_run_id
                .as_deref()
                .is_some_and(|id| id != request.run_id)
        {
            return Err(invalid("invalid question invocation"));
        }
        Ok(Self(Arc::new(Invocation {
            id: fresh_control_id()?,
            request,
            adapter,
            expires_at: now + QUESTION_TIMEOUT,
            state: Mutex::new(State {
                status: QuestionHookStatus::Pending,
                output: None,
                delivery_id: None,
                answers: None,
                native_send: NativeSendStatus::Unsent,
                send_rejection: None,
            }),
            guard: Mutex::new(guard::SendGuard::default()),
        })))
    }

    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn request(&self) -> &HookQuestionRequest {
        &self.0.request
    }
    pub fn adapter(&self) -> &NativeQuestion {
        &self.0.adapter
    }

    pub fn status(&self, now: Instant) -> QuestionHookStatus {
        let mut state = self.0.state.lock().expect("question state poisoned");
        self.expire(&mut state, now);
        state.status
    }

    /// Host calls only after durable intent and final authorization/run validation.
    pub fn submit_answer(
        &self,
        answers: &[QuestionAnswer],
        now: Instant,
    ) -> Result<bool, &'static str> {
        if !matches!(self.0.adapter, NativeQuestion::Claude(_)) {
            return Err("question_delivery_id_required");
        }
        self.submit_delivery(answers, self.id(), now)
    }

    pub fn submit_delivery(
        &self,
        answers: &[QuestionAnswer],
        delivery_id: &str,
        now: Instant,
    ) -> Result<bool, &'static str> {
        if delivery_id.trim().is_empty() || delivery_id.len() > 256 {
            return Err("invalid_delivery_id");
        }
        let mut output = self.0.adapter.answer_output(answers)?;
        if !matches!(self.0.adapter, NativeQuestion::Claude(_)) {
            output["delivery_id"] = delivery_id.into();
        }
        let mut state = self.0.state.lock().expect("question state poisoned");
        self.expire(&mut state, now);
        if state.status != QuestionHookStatus::Pending {
            return Ok(false);
        }
        state.status = QuestionHookStatus::Ready;
        state.output = Some(output);
        state.delivery_id = Some(delivery_id.into());
        state.answers = Some(answers.to_vec());
        Ok(true)
    }

    pub fn cancel(&self) {
        self.try_cancel();
    }

    /// The outcome and cancellation use the same lock as the first provider write.
    pub fn try_cancel(&self) -> bool {
        let mut state = self.0.state.lock().expect("question state poisoned");
        let revocable = match self.0.adapter {
            NativeQuestion::Claude(_) => state.status != QuestionHookStatus::Responded,
            NativeQuestion::Codex { .. }
            | NativeQuestion::OpenCode { .. }
            | NativeQuestion::Cursor { .. } => state.native_send == NativeSendStatus::Unsent,
        };
        if revocable {
            state.status = QuestionHookStatus::Cancelled;
            state.output = None;
        }
        revocable
    }

    pub fn take_response(&self, now: Instant) -> Option<Value> {
        let mut state = self.0.state.lock().expect("question state poisoned");
        self.expire(&mut state, now);
        if state.status != QuestionHookStatus::Ready {
            return None;
        }
        state.status = QuestionHookStatus::Responding;
        state.output.take()
    }

    /// Perform one bounded nonblocking write under the cancellation lock. The final byte and
    /// Responded transition are atomic to observers; a fast tool receipt cannot race this marker.
    pub fn with_response_write(
        &self,
        now: Instant,
        write: impl FnOnce() -> io::Result<bool>,
    ) -> io::Result<bool> {
        self.guarded_send(matches!(self.0.adapter, NativeQuestion::Claude(_)), || {
            let mut state = self.0.state.lock().expect("question state poisoned");
            self.expire(&mut state, now);
            if state.status != QuestionHookStatus::Responding {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "question response revoked",
                ));
            }
            let result = write();
            if matches!(self.0.adapter, NativeQuestion::Claude(_)) {
                state.native_send = match &result {
                    Ok(true) => NativeSendStatus::Sent,
                    Ok(false) => NativeSendStatus::Writing,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => state.native_send,
                    Err(_) => NativeSendStatus::Unknown,
                };
            }
            if matches!(result, Ok(true)) {
                state.status = QuestionHookStatus::Responded;
            }
            result
        })
        .map_err(|reason| io::Error::new(io::ErrorKind::PermissionDenied, reason))?
    }

    pub fn matches_delivery(&self, delivery_id: &str, answers: &[QuestionAnswer]) -> bool {
        let state = self.0.state.lock().expect("question state poisoned");
        state.delivery_id.as_deref() == Some(delivery_id)
            && state.answers.as_deref() == Some(answers)
    }

    pub fn native_send_status(&self) -> NativeSendStatus {
        self.0
            .state
            .lock()
            .expect("question state poisoned")
            .native_send
    }

    pub fn transport_sent(&self) -> bool {
        let state = self.0.state.lock().expect("question state poisoned");
        match self.0.adapter {
            NativeQuestion::Claude(_) => state.status == QuestionHookStatus::Responded,
            NativeQuestion::Codex { .. }
            | NativeQuestion::OpenCode { .. }
            | NativeQuestion::Cursor { .. } => state.native_send == NativeSendStatus::Sent,
        }
    }

    pub fn owns_consumption(&self, raw: &Value, delivery_id: &str) -> bool {
        let state = self.0.state.lock().expect("question state poisoned");
        state.delivery_id.as_deref() == Some(delivery_id)
            && match self.0.adapter {
                NativeQuestion::Claude(_) => state.status == QuestionHookStatus::Responded,
                NativeQuestion::OpenCode { .. } | NativeQuestion::Cursor { .. } => {
                    state.native_send == NativeSendStatus::Sent
                        && raw["delivery_id"].as_str() == Some(delivery_id)
                }
                NativeQuestion::Codex { .. } => {
                    matches!(
                        state.native_send,
                        NativeSendStatus::Sent | NativeSendStatus::Unknown
                    ) && raw["delivery_id"].as_str() == Some(delivery_id)
                }
            }
    }

    /// Call on the background host after its final GET/identity check, immediately before POST.
    pub fn begin_http_delivery(
        &self,
        delivery_id: &str,
        now: Instant,
    ) -> Result<NativeHttpDelivery, &'static str> {
        self.guarded_send(true, || {
            let mut state = self.0.state.lock().expect("question state poisoned");
            self.expire(&mut state, now);
            if !matches!(self.0.adapter, NativeQuestion::OpenCode { .. })
                || state.delivery_id.as_deref() != Some(delivery_id)
                || state.status != QuestionHookStatus::Responding
                || state.native_send != NativeSendStatus::Unsent
            {
                return Err("question_send_revoked");
            }
            state.native_send = NativeSendStatus::Writing;
            Ok(NativeHttpDelivery {
                ticket: self.clone(),
                finished: false,
            })
        })?
    }

    /// Shared by the owning relay and Runtime. Cancellation wins until the first bounded write.
    /// Once bytes may have escaped, only this delivery may flush; an IO error is never retryable.
    pub fn with_native_send(
        &self,
        delivery_id: &str,
        now: Instant,
        write: impl FnOnce() -> io::Result<bool>,
    ) -> io::Result<bool> {
        self.with_native_send_prepared(delivery_id, now, || Ok(()), |_| write())
    }

    /// Pure qualification runs under the cancellation lock before any bytes may escape.
    pub fn with_native_send_prepared<T>(
        &self,
        delivery_id: &str,
        now: Instant,
        prepare: impl FnOnce() -> Result<T, &'static str>,
        write: impl FnOnce(T) -> io::Result<bool>,
    ) -> io::Result<bool> {
        // An accepted Codex frame must finish flushing even after permission is revoked.
        let first = self.native_send_status() == NativeSendStatus::Unsent;
        self.guarded_send(first, || {
            let mut state = self.0.state.lock().expect("question state poisoned");
            self.expire(&mut state, now);
            if !matches!(
                self.0.adapter,
                NativeQuestion::Codex { .. } | NativeQuestion::Cursor { .. }
            ) || state.delivery_id.as_deref() != Some(delivery_id)
                || !matches!(
                    state.status,
                    QuestionHookStatus::Responding | QuestionHookStatus::Responded
                )
                || !matches!(
                    state.native_send,
                    NativeSendStatus::Unsent | NativeSendStatus::Writing
                )
            {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "question send revoked",
                ));
            }
            let prepared = prepare()
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
            state.native_send = NativeSendStatus::Writing;
            let result = write(prepared);
            state.native_send = match &result {
                Ok(true) => NativeSendStatus::Sent,
                Ok(false) => NativeSendStatus::Writing,
                Err(_) => NativeSendStatus::Unknown,
            };
            result
        })
        .map_err(|reason| io::Error::new(io::ErrorKind::PermissionDenied, reason))?
    }

    /// Terminal steps are interruptible between writes. A partial operation is never replayable.
    pub fn stop_terminal_send(&self) {
        if !matches!(self.0.adapter, NativeQuestion::Cursor { .. }) {
            return;
        }
        let mut state = self.0.state.lock().expect("question state poisoned");
        if state.native_send == NativeSendStatus::Writing {
            state.native_send = NativeSendStatus::Unknown;
        }
        state.status = QuestionHookStatus::Cancelled;
        state.output = None;
    }

    fn expire(&self, state: &mut State, now: Instant) {
        // RPC/HTTP requests live until their owning transport observes cancellation or
        // consumption. The 120s deadline belongs to the blocking Claude Hook, not to
        // the time a user may spend reading a native question. Authorization expires
        // separately at the host's final send gate.
        if matches!(
            self.0.adapter,
            NativeQuestion::Codex { .. } | NativeQuestion::OpenCode { .. }
        ) {
            return;
        }
        if !(matches!(self.0.adapter, NativeQuestion::Claude(_))
            && state.status == QuestionHookStatus::Responded)
            && (state.native_send == NativeSendStatus::Unsent
                || matches!(self.0.adapter, NativeQuestion::Claude(_))
                    && state.native_send == NativeSendStatus::Writing)
            && now >= self.0.expires_at
            && matches!(
                state.status,
                QuestionHookStatus::Pending
                    | QuestionHookStatus::Ready
                    | QuestionHookStatus::Responding
                    | QuestionHookStatus::Responded
            )
        {
            state.status = QuestionHookStatus::Cancelled;
            state.output = None;
        }
    }
}
