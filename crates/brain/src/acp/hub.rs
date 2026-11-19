//! 幕僚共用一条 ACP 连接。每个会话有自己的信箱；取消只发 session/cancel。
use super::client::{AcpError, SessionOptions};
use super::handshake::handshake;
use super::route::Switchboard;
use super::wire::{spawn_child, Incoming, Rpc, Spawned};
use crate::backend::{shared_staff_meta, BackendKind};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TOKEN_VAR: &str = "NORMA_BRAIN_TOKEN";

pub(crate) struct Port {
    session_id: String,
    rx: Receiver<Incoming>,
    write: Arc<dyn Fn(Value) -> io::Result<()> + Send + Sync>,
    board: Arc<Mutex<Switchboard>>,
    next_id: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
}

impl Port {
    pub(crate) fn cancel(&self) {
        let _ = (self.write)(json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {"sessionId": self.session_id}
        }));
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        if let Ok(mut board) = self.board.lock() {
            board.detach(&self.session_id);
        }
    }
}

impl Rpc for Port {
    fn send_request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.board
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .bind(id, &self.session_id);
        (self.write)(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        Ok(id)
    }

    fn respond(&mut self, id: &Value, result: Value) -> io::Result<()> {
        (self.write)(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    fn respond_error(&mut self, id: &Value, code: i64, message: &str) -> io::Result<()> {
        (self.write)(
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        )
    }

    fn recv(&mut self, timeout: Duration) -> Result<Incoming, RecvTimeoutError> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err(RecvTimeoutError::Disconnected);
        }
        self.rx.recv_timeout(timeout)
    }
}

/// 不启动进程的连接，测试用来确认取消和权限不会串会话。
#[cfg(test)]
pub(crate) struct Pipe {
    board: Arc<Mutex<Switchboard>>,
    sent: Arc<Mutex<Vec<Value>>>,
    next_id: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
}

#[cfg(test)]
impl Pipe {
    pub(crate) fn new() -> Self {
        Self {
            board: Arc::new(Mutex::new(Switchboard::new())),
            sent: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            alive: Arc::new(AtomicBool::new(true)),
        }
    }

    pub(crate) fn attach(&self, session: &str) -> Port {
        let rx = self
            .board
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .attach(session);
        let sent = self.sent.clone();
        Port {
            session_id: session.to_owned(),
            rx,
            write: Arc::new(move |value| {
                sent.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(value);
                Ok(())
            }),
            board: self.board.clone(),
            next_id: self.next_id.clone(),
            alive: self.alive.clone(),
        }
    }

    pub(crate) fn deliver(&self, incoming: Incoming) -> bool {
        self.board
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .deliver(incoming)
    }

    pub(crate) fn sent(&self) -> Vec<Value> {
        self.sent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

struct Live {
    child: std::process::Child,
    stdin: Arc<Mutex<ChildStdin>>,
    board: Arc<Mutex<Switchboard>>,
    next_id: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    initialized: bool,
    _adapter: Option<super::codex_compat::Adapter>,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Claude / Codex 幕僚共用的一条连接。OpenCode 不走这里。
pub struct SharedHub {
    kind: BackendKind,
    program: std::path::PathBuf,
    args: Vec<String>,
    cwd: std::path::PathBuf,
    env: Vec<(String, String)>,
    live: Mutex<Option<Live>>,
    opening: Mutex<()>,
    checked: AtomicBool,
}

impl SharedHub {
    pub fn prepare(
        kind: BackendKind,
        program: std::path::PathBuf,
        args: Vec<String>,
        cwd: std::path::PathBuf,
        env: Vec<(String, String)>,
    ) -> Option<Arc<Self>> {
        if kind == BackendKind::OpenCode {
            return None;
        }
        Some(Arc::new(Self {
            kind,
            program,
            args,
            cwd,
            env: crate::staff::gate::staff_env(&env, None),
            live: Mutex::new(None),
            opening: Mutex::new(()),
            checked: AtomicBool::new(false),
        }))
    }

    /// 自检不过就返回错误，调用方退回每个任务一个进程。
    pub fn open_staff(
        &self,
        options: &SessionOptions,
        cancel: &AtomicBool,
    ) -> Result<super::client::AcpSession, AcpError> {
        let _opening = self
            .opening
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // 自检只在拿到 opening 锁后跑一次；失败不记死，下一个会话再试，这一个先退回独占进程。
        if !self.checked.load(Ordering::Relaxed) {
            if !self.agents_ok(options, cancel) {
                return Err(AcpError::Closed);
            }
            self.checked.store(true, Ordering::Relaxed);
        }
        self.ensure().map_err(AcpError::Spawn)?;
        let initialize = {
            let live = self
                .live
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let live = live.as_ref().ok_or(AcpError::Closed)?;
            !live.initialized
        };
        let mut probe = self
            .port_for(&format!("hs-{}", uuid::Uuid::new_v4().simple()))
            .map_err(|_| AcpError::Closed)?;
        let options = with_token(options);
        let (session_id, created) =
            handshake(&mut probe, &options, cancel, true, initialize, &mut || {
                if let Some(live) = self
                    .live
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_mut()
                {
                    live.initialized = true;
                }
            })?;
        drop(probe);
        let port = self.port_for(&session_id).map_err(|_| AcpError::Closed)?;
        Ok(super::client::AcpSession::from_shared(
            port,
            session_id,
            created,
            options.cwd.clone(),
            options.staff_shell.clone(),
        ))
    }

    fn agents_ok(&self, options: &SessionOptions, cancel: &AtomicBool) -> bool {
        let Some(bin) = options.staff_shell.as_deref() else {
            return false;
        };
        let token = options
            .env
            .iter()
            .find(|(key, _)| key == TOKEN_VAR)
            .map(|(_, value)| value.as_str())
            .unwrap_or("");
        agents_ready(bin, token, &options.cwd, cancel)
    }

    fn ensure(&self) -> io::Result<()> {
        let mut slot = self
            .live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot
            .as_ref()
            .is_some_and(|live| live.alive.load(Ordering::Relaxed))
        {
            return Ok(());
        }
        *slot = None;
        *slot = Some(start_live(
            &self.program,
            &self.args,
            &self.cwd,
            &self.env,
            self.kind,
        )?);
        Ok(())
    }

    fn port_for(&self, session: &str) -> io::Result<Port> {
        let live = self
            .live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let live = live
            .as_ref()
            .ok_or_else(|| io::Error::other("no adapter"))?;
        if !live.alive.load(Ordering::Relaxed) {
            return Err(io::Error::other("adapter exited"));
        }
        let rx = live
            .board
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .attach(session);
        let stdin = live.stdin.clone();
        Ok(Port {
            session_id: session.to_owned(),
            rx,
            write: Arc::new(move |value| write_line(&stdin, &value)),
            board: live.board.clone(),
            next_id: live.next_id.clone(),
            alive: live.alive.clone(),
        })
    }
}

fn with_token(options: &SessionOptions) -> SessionOptions {
    let mut options = options.clone();
    let Some(token) = options
        .env
        .iter()
        .find(|(key, _)| key == TOKEN_VAR)
        .map(|(_, value)| value.clone())
    else {
        return options;
    };
    if let Some(meta) = shared_staff_meta(options.kind, options.model.as_deref(), &token) {
        options.meta = Some(meta);
    }
    options
}

/// 自检最多等这么久；norma 卡住不能拖住会话创建。
const AGENTS_CHECK_LIMIT: Duration = Duration::from_secs(5);

pub(crate) fn agents_ready(bin: &Path, token: &str, cwd: &Path, cancel: &AtomicBool) -> bool {
    agents_ready_within(bin, token, cwd, cancel, AGENTS_CHECK_LIMIT)
}

fn agents_ready_within(
    bin: &Path,
    token: &str,
    cwd: &Path,
    cancel: &AtomicBool,
    limit: Duration,
) -> bool {
    if token.is_empty() || !bin.is_file() {
        return false;
    }
    let Ok(mut child) = std::process::Command::new(bin)
        .args(["brain", "agents"])
        .env(TOKEN_VAR, token)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => return false,
        }
        if cancel.load(Ordering::Relaxed) || std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn start_live(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    kind: BackendKind,
) -> io::Result<Live> {
    let Spawned {
        child,
        stdin,
        stdout,
        adapter,
    } = spawn_child(program, args, cwd, env, kind)?;
    let board = Arc::new(Mutex::new(Switchboard::new()));
    let alive = Arc::new(AtomicBool::new(true));
    let stdin = Arc::new(Mutex::new(stdin));
    let reader_board = board.clone();
    let reader_alive = alive.clone();
    let reader_stdin = stdin.clone();
    std::thread::spawn(move || {
        let lines = std::io::BufReader::new(stdout).lines();
        for line in lines {
            let Ok(line) = line else { break };
            let Some(message) = super::wire::parse(&line) else {
                continue;
            };
            let request_id = match &message {
                super::wire::Incoming::Request { id, .. } => Some(id.clone()),
                _ => None,
            };
            let delivered = reader_board
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .deliver(message);
            // 对不上会话的请求不能悄悄吞掉，否则适配器会一直等响应。
            if let (false, Some(id)) = (delivered, request_id) {
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32000, "message": "no session for this request"}
                });
                let _ = write_line(&reader_stdin, &reply);
            }
        }
        reader_alive.store(false, Ordering::Relaxed);
    });
    Ok(Live {
        child,
        stdin,
        board,
        next_id: Arc::new(AtomicU64::new(1)),
        alive,
        initialized: false,
        _adapter: adapter,
    })
}

fn write_line(stdin: &Mutex<ChildStdin>, value: &Value) -> io::Result<()> {
    let mut line = value.to_string();
    line.push('\n');
    let mut stdin = stdin
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    stdin.write_all(line.as_bytes())?;
    stdin.flush()
}

#[cfg(test)]
#[path = "../../tests/unit/acp_hub_tests.rs"]
mod tests;
