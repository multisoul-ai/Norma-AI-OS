use super::{http, AgentHookRegistry, CLIENT_RECV_INTERVAL};
use crossbeam_channel::Receiver;
use norma::agent_hook::{ApprovalStatus, HookApproval, HookQuestion, QuestionHookStatus};
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

pub(super) const CAPACITY: usize = 64;

#[cfg(test)]
#[path = "../../../tests/unit/agent_hook_question_write_tests.rs"]
mod tests;

enum Control {
    Approval(HookApproval),
    Question(HookQuestion),
}

impl Control {
    fn cancel(&self) {
        match self {
            Self::Approval(ticket) => ticket.cancel(),
            Self::Question(ticket) => ticket.cancel(),
        }
    }
    fn cancelled(&self, now: Instant) -> bool {
        match self {
            Self::Approval(ticket) => ticket.status(now) == ApprovalStatus::Cancelled,
            Self::Question(ticket) => ticket.status(now) == QuestionHookStatus::Cancelled,
        }
    }
    fn take_response(&self, now: Instant) -> Option<serde_json::Value> {
        match self {
            Self::Approval(ticket) => ticket.take_response(now),
            Self::Question(ticket) => ticket.take_response(now),
        }
    }
}

pub(super) struct WaitingHook {
    stream: TcpStream,
    token: String,
    control: Control,
    output: Vec<u8>,
    written: usize,
    sending_answer: bool,
}

impl Drop for WaitingHook {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl WaitingHook {
    pub(super) fn new(
        stream: TcpStream,
        token: String,
        approval: HookApproval,
    ) -> std::io::Result<Self> {
        Self::with_control(stream, token, Control::Approval(approval))
    }

    pub(super) fn new_question(
        stream: TcpStream,
        token: String,
        question: HookQuestion,
    ) -> std::io::Result<Self> {
        Self::with_control(stream, token, Control::Question(question))
    }

    fn with_control(stream: TcpStream, token: String, control: Control) -> std::io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            token,
            control,
            output: Vec::new(),
            written: 0,
            sending_answer: false,
        })
    }

    /// Nonblocking: one waiting Hook must never occupy an ordinary HTTP worker.
    fn poll(&mut self, registry: &Arc<Mutex<AgentHookRegistry>>) -> bool {
        match self.stream.peek(&mut [0_u8; 1]) {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            _ => return false, // EOF, unexpected data or error: this invocation is gone.
        }
        // Keep route revocation ordered with this bounded nonblocking write.
        let registry = registry.lock().expect("hook registry poisoned");
        let revoked = !registry.routes.contains_key(&self.token);
        if revoked {
            self.control.cancel();
        }
        let now = Instant::now();
        if revoked || self.control.cancelled(now) {
            if self.written > 0 {
                return false;
            }
            self.output = http::response("204 No Content", &[]);
            self.sending_answer = false;
        } else if self.output.is_empty() {
            if let Some(output) = self.control.take_response(now) {
                self.output = http::response("200 OK", output.to_string().as_bytes());
                self.sending_answer = true;
            } else {
                return true;
            }
        }
        let mut write = || {
            let size = self.stream.write(&self.output[self.written..])?;
            if size == 0 {
                return Err(std::io::Error::from(ErrorKind::WriteZero));
            }
            self.written += size;
            Ok(self.written == self.output.len())
        };
        let result = match &self.control {
            Control::Question(ticket) if self.sending_answer => {
                ticket.with_response_write(now, write)
            }
            _ => write(),
        };
        match result {
            Ok(complete) => !complete,
            Err(error) if error.kind() == ErrorKind::WouldBlock => true,
            Err(_) => false,
        }
    }
}

pub(super) fn run(
    receiver: Receiver<WaitingHook>,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
) {
    // ponytail: one bounded O(n) poll loop; use readiness IO if >64 concurrent Hooks is needed.
    let mut waiting: Vec<WaitingHook> = Vec::new();
    while !shutdown.load(Ordering::Acquire) {
        if waiting.len() < CAPACITY {
            if let Ok(hook) = receiver.recv_timeout(CLIENT_RECV_INTERVAL) {
                waiting.push(hook);
            }
            while waiting.len() < CAPACITY {
                match receiver.try_recv() {
                    Ok(hook) => waiting.push(hook),
                    Err(_) => break,
                }
            }
        } else {
            std::thread::sleep(CLIENT_RECV_INTERVAL);
        }
        waiting.retain_mut(|hook| hook.poll(&registry));
    }
    // Dropping both active and buffered sockets cancels all associated tickets.
}
