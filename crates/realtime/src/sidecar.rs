use crate::protocol::{decode_event, encode_command, RealtimeCommand, RealtimeEvent};
use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Eq, PartialEq)]
pub struct SecretString {
    value: String,
}

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_env_value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([redacted])")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct SidecarLaunchConfig {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub openai_api_key: SecretString,
}

impl fmt::Debug for SidecarLaunchConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SidecarLaunchConfig")
            .field("executable", &self.executable)
            .field("args", &self.args)
            .field("openai_api_key", &self.openai_api_key)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RealtimeSidecarError {
    #[error("failed to launch realtime sidecar `{executable}`")]
    LaunchFailed {
        executable: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("realtime sidecar process missing {pipe_name} pipe")]
    MissingPipe { pipe_name: &'static str },
    #[error("failed to encode realtime sidecar command")]
    CommandEncodeFailed,
    #[error("failed to write realtime sidecar command")]
    CommandWriteFailed(#[source] io::Error),
    #[error("malformed realtime sidecar event")]
    MalformedEvent,
    #[error("failed to read realtime sidecar stdout")]
    StdoutReadFailed(#[source] io::Error),
    #[error("realtime sidecar stdout closed before a terminal event")]
    StdoutClosed,
    #[error("failed to wait for realtime sidecar process")]
    WaitFailed(#[source] io::Error),
    #[error("failed to kill realtime sidecar process after stop timeout")]
    KillFailed(#[source] io::Error),
    #[error("realtime sidecar process did not exit within {timeout:?}")]
    StopTimedOut { timeout: Duration },
}

pub struct SidecarProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<Result<RealtimeEvent, RealtimeSidecarError>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for SidecarProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SidecarProcess")
            .field("child_id", &self.child.id())
            .field("stdin_open", &self.stdin.is_some())
            .finish_non_exhaustive()
    }
}

impl SidecarProcess {
    pub fn spawn(config: SidecarLaunchConfig) -> Result<Self, RealtimeSidecarError> {
        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .env("OPENAI_API_KEY", config.openai_api_key.as_env_value())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|source| RealtimeSidecarError::LaunchFailed {
                executable: config.executable,
                source,
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or(RealtimeSidecarError::MissingPipe { pipe_name: "stdin" })?;
        let stdout = child
            .stdout
            .take()
            .ok_or(RealtimeSidecarError::MissingPipe {
                pipe_name: "stdout",
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or(RealtimeSidecarError::MissingPipe {
                pipe_name: "stderr",
            })?;
        let (sender, events) = unbounded();

        let stdout_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => match decode_event(&line) {
                        Ok(RealtimeEvent::IgnoredUnknown { .. }) => {}
                        Ok(event) => {
                            if sender.send(Ok(event)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            if sender
                                .send(Err(RealtimeSidecarError::MalformedEvent))
                                .is_err()
                            {
                                break;
                            }
                        }
                    },
                    Err(error) => {
                        let _ = sender.send(Err(RealtimeSidecarError::StdoutReadFailed(error)));
                        break;
                    }
                }
            }
        });
        let stderr_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut sink = io::sink();
            let _ = io::copy(&mut reader, &mut sink);
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            events,
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
        })
    }

    pub fn send(&mut self, command: &RealtimeCommand) -> Result<(), RealtimeSidecarError> {
        let line =
            encode_command(command).map_err(|_| RealtimeSidecarError::CommandEncodeFailed)?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or(RealtimeSidecarError::CommandEncodeFailed)?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(RealtimeSidecarError::CommandWriteFailed)
    }

    pub fn try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(RealtimeSidecarError::StdoutClosed)),
        }
    }

    pub fn stop(mut self, timeout: Duration) -> Result<(), RealtimeSidecarError> {
        self.stdin.take();
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .child
                .try_wait()
                .map_err(RealtimeSidecarError::WaitFailed)?
                .is_some()
            {
                self.join_reader_threads();
                return Ok(());
            }

            if Instant::now() >= deadline {
                self.child
                    .kill()
                    .map_err(RealtimeSidecarError::KillFailed)?;
                let _ = self.child.wait();
                self.join_reader_threads();
                return Err(RealtimeSidecarError::StopTimedOut { timeout });
            }

            thread::sleep(Duration::from_millis(10));
        }
    }

    fn join_reader_threads(&mut self) {
        if let Some(thread) = self.stdout_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
    }
}
