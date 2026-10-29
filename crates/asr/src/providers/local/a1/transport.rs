//! Bounded duplex socket transport; all calls and Drop run on an ASR/background thread.
use super::config::error;
use crate::{AsrError, RecognitionCancellation};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default, Clone)]
pub(super) struct Guard {
    pub host: RecognitionCancellation,
    pub session: RecognitionCancellation,
}
impl Guard {
    pub fn check(&self) -> Result<(), AsrError> {
        if self.host.is_cancelled() || self.session.is_cancelled() {
            Err(AsrError::SessionClosed)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
pub(super) struct ProcessControl(Arc<Mutex<Child>>);
impl ProcessControl {
    pub fn terminate(&self) {
        let mut child = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        unsafe {
            libc::killpg(child.id() as i32, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    #[cfg(test)]
    pub(super) fn exited(&self) -> bool {
        self.0.lock().unwrap().try_wait().unwrap().is_some()
    }
    #[cfg(test)]
    fn id(&self) -> u32 {
        self.0.lock().unwrap().id()
    }
}

pub(super) struct Transport {
    child: ProcessControl,
    socket: UnixStream,
    role: &'static str,
}

impl Transport {
    #[cfg(test)]
    pub(super) fn control(&self) -> ProcessControl {
        self.child.clone()
    }
    #[cfg(test)]
    pub fn spawn(
        command: Command,
        role: &'static str,
        guard: &Guard,
        timeout: Duration,
    ) -> Result<Self, AsrError> {
        Self::spawn_registered(command, role, guard, timeout, |_| {})
    }
    pub fn spawn_registered(
        mut command: Command,
        role: &'static str,
        guard: &Guard,
        timeout: Duration,
        register: impl FnOnce(ProcessControl),
    ) -> Result<Self, AsrError> {
        guard.check()?;
        let (socket, child_socket) = UnixStream::pair().map_err(error)?;
        socket.set_nonblocking(true).map_err(error)?;
        let input: OwnedFd = child_socket.try_clone().map_err(error)?.into();
        let output: OwnedFd = child_socket.into();
        let child = command
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(error)?;
        let child = ProcessControl(Arc::new(Mutex::new(child)));
        register(child.clone());
        let mut worker = Self {
            child,
            socket,
            role,
        };
        let ready = worker.read_response(guard, Instant::now() + timeout)?;
        if ready["v"] != 1 || ready["op"] != "ready" || ready["source"] != role {
            return Err(error("worker handshake mismatch"));
        }
        Ok(worker)
    }

    pub fn request(
        &mut self,
        session: u64,
        seq: u64,
        op: &str,
        pcm: &[u8],
        guard: &Guard,
        timeout: Duration,
    ) -> Result<String, AsrError> {
        if pcm.len() > 32_000 || !pcm.len().is_multiple_of(2) {
            return Err(AsrError::InvalidPcm);
        }
        let header = serde_json::to_vec(
            &json!({"v":1,"session":session,"seq":seq,"op":op,"bytes":pcm.len()}),
        )
        .map_err(error)?;
        let deadline = Instant::now() + timeout;
        self.write_all(&(header.len() as u32).to_le_bytes(), guard, deadline)?;
        self.write_all(&header, guard, deadline)?;
        self.write_all(pcm, guard, deadline)?;
        let response = self.read_response(guard, deadline)?;
        if response["v"] != 1
            || response["session"] != session
            || response["seq"] != seq
            || response["op"] != op
            || response["source"] != self.role
            || response["fallback"] != false
        {
            return Err(error("stale, degraded or invalid worker response"));
        }
        response["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| error("missing worker text"))
    }

    fn read_response(&mut self, guard: &Guard, deadline: Instant) -> Result<Value, AsrError> {
        let mut prefix = [0; 4];
        self.read_all(&mut prefix, guard, deadline)?;
        let length = u32::from_le_bytes(prefix) as usize;
        if length == 0 || length > 65_536 {
            return Err(error("oversized worker response"));
        }
        let mut bytes = vec![0; length];
        self.read_all(&mut bytes, guard, deadline)?;
        serde_json::from_slice(&bytes).map_err(error)
    }

    fn read_all(
        &mut self,
        mut buffer: &mut [u8],
        guard: &Guard,
        deadline: Instant,
    ) -> Result<(), AsrError> {
        while !buffer.is_empty() {
            guard.check()?;
            if Instant::now() >= deadline {
                return Err(error("worker timeout"));
            }
            match self.socket.read(buffer) {
                Ok(0) => return Err(error("worker disconnected")),
                Ok(count) => buffer = &mut buffer[count..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(error(e)),
            }
        }
        guard.check()
    }

    fn write_all(
        &mut self,
        mut bytes: &[u8],
        guard: &Guard,
        deadline: Instant,
    ) -> Result<(), AsrError> {
        while !bytes.is_empty() {
            guard.check()?;
            if Instant::now() >= deadline {
                return Err(error("worker write timeout"));
            }
            match self.socket.write(bytes) {
                Ok(0) => return Err(error("worker disconnected during write")),
                Ok(count) => bytes = &bytes[count..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(error(e)),
            }
        }
        guard.check()
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        // The command starts a fresh process group. Never signal another task's PID/group.
        self.child.terminate();
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/a1_transport_tests.rs"]
mod tests;
