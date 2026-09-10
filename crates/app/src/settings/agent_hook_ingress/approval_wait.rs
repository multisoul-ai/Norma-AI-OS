use super::{http, AgentHookRegistry, CLIENT_RECV_INTERVAL};
use crossbeam_channel::Receiver;
use norma::agent_hook::{ApprovalStatus, HookApproval};
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

pub(super) const CAPACITY: usize = 64;

pub(super) struct WaitingHook {
    stream: TcpStream,
    token: String,
    approval: HookApproval,
    output: Vec<u8>,
    written: usize,
}

impl Drop for WaitingHook {
    fn drop(&mut self) {
        self.approval.cancel();
    }
}

impl WaitingHook {
    pub(super) fn new(
        stream: TcpStream,
        token: String,
        approval: HookApproval,
    ) -> std::io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            token,
            approval,
            output: Vec::new(),
            written: 0,
        })
    }

    /// Nonblocking: one waiting Hook must never occupy an ordinary HTTP worker.
    fn poll(&mut self, registry: &Arc<Mutex<AgentHookRegistry>>) -> bool {
        match self.stream.peek(&mut [0_u8; 1]) {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            _ => return false, // EOF, unexpected data or error: this invocation is gone.
        }
        if !registry
            .lock()
            .expect("hook registry poisoned")
            .routes
            .contains_key(&self.token)
        {
            self.approval.cancel();
        }
        if self.output.is_empty() {
            let now = Instant::now();
            if self.approval.status(now) == ApprovalStatus::Cancelled {
                self.output = http::response("204 No Content", &[]);
            } else if let Some(output) = self.approval.take_response(now) {
                self.output = http::response("200 OK", output.to_string().as_bytes());
            } else {
                return true;
            }
        }
        match self.stream.write(&self.output[self.written..]) {
            Ok(0) => false,
            Ok(size) => {
                self.written += size;
                self.written < self.output.len()
            }
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
