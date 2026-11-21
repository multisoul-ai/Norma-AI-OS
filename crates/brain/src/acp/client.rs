//! 一个 ACP 会话：握手、创建会话、发 prompt，并处理期间的权限请求。

use super::handshake::handshake;
use super::hub::Port;
use super::permission;
use super::trace::{classify_update, Merger, TraceItem};
use super::wire::{Connection, Incoming, Rpc};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("无法启动适配器：{0}")]
    Spawn(std::io::Error),
    #[error("适配器已退出")]
    Closed,
    #[error("适配器返回错误：{0}")]
    Remote(String),
    #[error("等待适配器超时")]
    Timeout,
    #[error("已取消")]
    Cancelled,
    /// 后端报告线程处于 systemError（如 429 限流）：这一轮没有真正完成，值得重试。
    #[error("后端临时故障：{0}")]
    Transient(String),
}

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub kind: crate::backend::BackendKind,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// 放进 `session/new` 的 `_meta`，用于锁定权限模式和禁用写工具。
    pub meta: Option<Value>,
    pub mode: Option<String>,
    /// 建好会话后再用 `session/set_config_option` 切一次模型：适配器会让用户自己的
    /// Claude Code 设置覆盖 `_meta` 里的模型。
    pub model: Option<String>,
    /// Codex 的推理强度，用 `session/set_config_option` 的 `reasoning_effort` 设置。
    pub effort: Option<String>,
    pub handshake_timeout: Duration,
    /// 幕僚会话才设置。有值时权限回调只放行这个绝对路径下的 `norma brain`。
    pub staff_shell: Option<PathBuf>,
}

enum SessionLink {
    Owned(Connection),
    Shared(Port),
}

impl Rpc for SessionLink {
    fn send_request(&mut self, method: &str, params: Value) -> std::io::Result<u64> {
        match self {
            Self::Owned(conn) => conn.send_request(method, params),
            Self::Shared(port) => port.send_request(method, params),
        }
    }

    fn respond(&mut self, id: &Value, result: Value) -> std::io::Result<()> {
        match self {
            Self::Owned(conn) => conn.respond(id, result),
            Self::Shared(port) => port.respond(id, result),
        }
    }

    fn respond_error(&mut self, id: &Value, code: i64, message: &str) -> std::io::Result<()> {
        match self {
            Self::Owned(conn) => conn.respond_error(id, code, message),
            Self::Shared(port) => port.respond_error(id, code, message),
        }
    }

    fn recv(&mut self, timeout: Duration) -> Result<Incoming, std::sync::mpsc::RecvTimeoutError> {
        match self {
            Self::Owned(conn) => conn.recv(timeout),
            Self::Shared(port) => port.recv(timeout),
        }
    }
}

pub struct AcpSession {
    conn: SessionLink,
    session_id: String,
    /// `session/new` 的回复，用来读出可选的模型。
    created: Value,
    questions: Option<super::questions::QuestionHost>,
    cwd: PathBuf,
    staff_shell: Option<PathBuf>,
    kind: crate::backend::BackendKind,
    /// 共用连接上被取消或超时的上一轮 prompt：它的响应和尾部通知还没排干。
    stale_prompt: Option<u64>,
}

impl AcpSession {
    pub fn start(options: &SessionOptions) -> Result<Self, AcpError> {
        Self::start_cancellable(options, &AtomicBool::new(false))
    }

    pub fn start_cancellable(
        options: &SessionOptions,
        cancel: &AtomicBool,
    ) -> Result<Self, AcpError> {
        Self::start_with_questions(options, cancel, false)
    }

    pub fn start_staff_cancellable(
        options: &SessionOptions,
        cancel: &AtomicBool,
    ) -> Result<Self, AcpError> {
        Self::start_with_questions(options, cancel, true)
    }

    fn start_with_questions(
        options: &SessionOptions,
        cancel: &AtomicBool,
        staff: bool,
    ) -> Result<Self, AcpError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(AcpError::Cancelled);
        }
        let mut conn = Connection::spawn(
            &options.program,
            &options.args,
            &options.cwd,
            &options.env,
            options.kind,
        )
        .map_err(AcpError::Spawn)?;
        let (session_id, created) = handshake(&mut conn, options, cancel, staff, true, &mut || {})?;
        Ok(Self {
            conn: SessionLink::Owned(conn),
            session_id,
            created,
            questions: None,
            cwd: options.cwd.clone(),
            staff_shell: options.staff_shell.clone(),
            kind: options.kind,
            stale_prompt: None,
        })
    }

    pub(crate) fn from_shared(
        port: Port,
        session_id: String,
        created: Value,
        cwd: PathBuf,
        staff_shell: Option<PathBuf>,
        kind: crate::backend::BackendKind,
    ) -> Self {
        Self {
            conn: SessionLink::Shared(port),
            session_id,
            created,
            questions: None,
            cwd,
            staff_shell,
            kind,
            stale_prompt: None,
        }
    }

    pub(crate) fn is_shared(&self) -> bool {
        matches!(self.conn, SessionLink::Shared(_))
    }

    pub fn set_question_host(&mut self, host: Option<super::questions::QuestionHost>) {
        self.questions = host;
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 这个会话里可选的模型。
    pub fn model_choices(&self) -> Vec<crate::backend::ModelChoice> {
        crate::backend::model_choices(&self.created)
    }

    /// 发一条 prompt，返回助手的文字回复。期间的权限请求一律放行。
    pub fn prompt(
        &mut self,
        text: &str,
        cancel: &AtomicBool,
        timeout: Duration,
        sink: &mut dyn FnMut(TraceItem),
    ) -> Result<String, AcpError> {
        self.prompt_observed(text, &[], cancel, Some(timeout), sink, &mut |_| {})
    }

    /// Observe raw progress before audit trace coalescing; observations are not persisted replies.
    /// `timeout == None` 时没有截止，只靠适配器结束或 `cancel`。
    pub fn prompt_observed(
        &mut self,
        text: &str,
        images: &[super::content::PromptImage],
        cancel: &AtomicBool,
        timeout: Option<Duration>,
        sink: &mut dyn FnMut(TraceItem),
        progress: &mut dyn FnMut(&TraceItem),
    ) -> Result<String, AcpError> {
        self.drain_stale()?;
        let started = Instant::now();
        let mut sent: Option<u64> = None;
        let staff_gate = self.staff_shell.clone().map(|bin| (bin, self.cwd.clone()));
        let mut merger = Merger::default();
        let mut stop_reason = None;
        let mut system_error = false;
        let result = (|| {
            if cancel.load(Ordering::Relaxed) {
                self.abort();
                return Err(AcpError::Cancelled);
            }
            let id = self
                .conn
                .send_request(
                    "session/prompt",
                    super::content::session_prompt(&self.session_id, text, images),
                )
                .map_err(|_| AcpError::Closed)?;
            sent = Some(id);
            let mut deadline = timeout.map(|limit| Instant::now() + limit);
            let mut tick = Instant::now();
            let mut reply = String::new();
            loop {
                let now = Instant::now();
                if let Some(questions) = &mut self.questions {
                    if questions.waiting() {
                        if let Some(deadline) = deadline.as_mut() {
                            *deadline += now.duration_since(tick);
                        }
                    }
                    questions.poll(&mut self.conn, false)?;
                }
                tick = now;
                if cancel.load(Ordering::Relaxed) {
                    self.abort();
                    return Err(AcpError::Cancelled);
                }
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    self.abort();
                    return Err(AcpError::Timeout);
                }
                let incoming = match self.conn.recv(Duration::from_millis(100)) {
                    Ok(incoming) => incoming,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err(AcpError::Closed),
                };
                if cancel.load(Ordering::Relaxed) {
                    self.abort();
                    return Err(AcpError::Cancelled);
                }
                let received_at = Instant::now();
                if self
                    .questions
                    .as_ref()
                    .is_some_and(|questions| questions.waiting())
                {
                    if let Some(deadline) = deadline.as_mut() {
                        *deadline += received_at.duration_since(tick);
                    }
                    tick = received_at;
                }
                if deadline.is_some_and(|deadline| received_at >= deadline) {
                    self.abort();
                    return Err(AcpError::Timeout);
                }
                match incoming {
                    Incoming::Response { id: rid, result } if rid == id => {
                        let stop = result
                            .as_ref()
                            .ok()
                            .and_then(|value| value["stopReason"].as_str())
                            .unwrap_or("error");
                        stop_reason = Some(if system_error { "error" } else { stop }.to_owned());
                        if stop == "cancelled" {
                            return Err(AcpError::Cancelled);
                        }
                        if system_error {
                            return Err(AcpError::Transient(reply.trim().to_owned()));
                        }
                        return result
                            .map(|_| reply)
                            .map_err(|e| AcpError::Remote(e.to_string()));
                    }
                    Incoming::Notification { method, params } if method == "session/update" => {
                        let update = &params["update"];
                        if update["_meta"]["codex"]["threadStatus"]["type"] == "systemError" {
                            system_error = true;
                        }
                        if update["sessionUpdate"] == "agent_message_chunk" {
                            reply.push_str(update["content"]["text"].as_str().unwrap_or_default());
                        }
                        if let Some(item) = classify_update(update) {
                            progress(&item);
                            for out in merger.push(item) {
                                sink(out);
                            }
                        }
                    }
                    Incoming::Request { id, method, params } => {
                        if let Some(item) = merger.flush() {
                            sink(item);
                        }
                        if method == "elicitation/create" {
                            if let Some(questions) = &mut self.questions {
                                if questions.open(&id, &params)? {
                                    continue;
                                }
                            }
                        }
                        permission::answer(
                            &mut self.conn,
                            &id,
                            &method,
                            &params,
                            staff_gate
                                .as_ref()
                                .map(|(bin, cwd)| (bin.as_path(), cwd.as_path())),
                            self.kind,
                            &mut |item| {
                                progress(&item);
                                sink(item);
                            },
                        )?;
                    }
                    _ => {}
                }
            }
        })();
        if self.is_shared()
            && stop_reason.is_none()
            && matches!(result, Err(AcpError::Cancelled | AcpError::Timeout))
        {
            self.stale_prompt = sent;
        }
        if let Some(questions) = &mut self.questions {
            questions.poll(&mut self.conn, true)?;
        }
        if let Some(item) = merger.flush() {
            sink(item);
        }
        let stop = match &result {
            Err(AcpError::Cancelled) => "cancelled",
            Err(AcpError::Timeout) => "timeout",
            _ => stop_reason.as_deref().unwrap_or("error"),
        };
        sink(TraceItem::turn_end(
            stop,
            started.elapsed().as_millis() as u64,
        ));
        result
    }

    /// 上一轮被取消后，等它的响应回来，丢掉尾部通知，并拒绝它遗留的权限请求；
    /// 否则这些尾巴会串进下一轮的回复。排不干说明适配器卡住了，让上层重建会话。
    fn drain_stale(&mut self) -> Result<(), AcpError> {
        let Some(old) = self.stale_prompt.take() else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.conn.recv(Duration::from_millis(100)) {
                Ok(Incoming::Response { id, .. }) if id == old => return Ok(()),
                Ok(Incoming::Request { id, .. }) => {
                    let _ = self.conn.respond_error(&id, -32000, "cancelled");
                }
                Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(AcpError::Closed),
            }
        }
        Err(AcpError::Closed)
    }

    fn abort(&mut self) {
        match &mut self.conn {
            // 共用连接上还有别的会话，只取消这一轮。
            SessionLink::Shared(port) => port.cancel(),
            SessionLink::Owned(conn) => {
                let _ = conn.notify("session/cancel", json!({"sessionId": self.session_id}));
                conn.kill();
            }
        }
    }
}

/// 建一次会话读出后端可选的模型，然后关掉。用于设置页列出 Codex、OpenCode 的模型。
pub fn discover_models(
    backend: &crate::backend::Backend,
    cwd: PathBuf,
    env: Vec<(String, String)>,
) -> Result<Vec<crate::backend::ModelChoice>, AcpError> {
    let options = SessionOptions {
        kind: backend.kind,
        program: backend.program.clone(),
        args: backend.args.clone(),
        cwd,
        env,
        meta: crate::backend::session_meta(backend.kind, None, false),
        mode: None,
        staff_shell: None,
        model: None,
        effort: None,
        handshake_timeout: Duration::from_secs(60),
    };
    Ok(AcpSession::start(&options)?.model_choices())
}
