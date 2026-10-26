//! 一个 ACP 会话：握手、创建会话、发 prompt，并处理期间的权限请求。

use super::gate::{pick_option, Gate, PermissionRequest, Verdict};
use super::trace::{classify_update, Merger, TraceItem, TraceKind};
use super::wire::{Connection, Incoming};
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
}

pub struct AcpSession {
    conn: Connection,
    session_id: String,
    /// `session/new` 的回复，用来读出可选的模型。
    created: Value,
}

impl AcpSession {
    pub fn start(options: &SessionOptions) -> Result<Self, AcpError> {
        let mut conn = Connection::spawn(
            &options.program,
            &options.args,
            &options.cwd,
            &options.env,
            options.kind,
        )
        .map_err(AcpError::Spawn)?;
        let deadline = Instant::now() + options.handshake_timeout;
        call(
            &mut conn,
            "initialize",
            json!({"protocolVersion": 1, "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false}}),
            deadline,
        )?;
        let mut params = json!({"cwd": options.cwd, "mcpServers": []});
        if let Some(meta) = &options.meta {
            params["_meta"] = meta.clone();
        }
        let created = call(&mut conn, "session/new", params, deadline)?;
        let session_id = created["sessionId"]
            .as_str()
            .ok_or_else(|| AcpError::Remote("session/new 没有返回 sessionId".into()))?
            .to_string();
        if let Some(mode) = &options.mode {
            call(
                &mut conn,
                "session/set_mode",
                json!({"sessionId": session_id, "modeId": mode}),
                deadline,
            )?;
        }
        let configs = [
            ("model", &options.model),
            ("reasoning_effort", &options.effort),
        ];
        for (config_id, value) in configs {
            let Some(value) = value else { continue };
            // 切不过去时沿用适配器的默认值，不让会话因此建不起来。
            if let Err(error) = call(
                &mut conn,
                "session/set_config_option",
                json!({"sessionId": session_id, "configId": config_id, "value": value}),
                deadline,
            ) {
                tracing::warn!(%error, config_id, value, "大脑会话设置失败");
            }
        }
        Ok(Self {
            conn,
            session_id,
            created,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 这个会话里可选的模型。
    pub fn model_choices(&self) -> Vec<crate::backend::ModelChoice> {
        crate::backend::model_choices(&self.created)
    }

    /// 发一条 prompt，返回助手的文字回复。期间的权限请求交给 `gate`。
    pub fn prompt(
        &mut self,
        text: &str,
        gate: &Gate,
        cancel: &AtomicBool,
        timeout: Duration,
        sink: &mut dyn FnMut(TraceItem),
    ) -> Result<String, AcpError> {
        let started = Instant::now();
        let mut merger = Merger::default();
        let mut stop_reason = None;
        let mut system_error = false;
        let result = (|| {
            let id = self
            .conn
            .send_request(
                "session/prompt",
                json!({"sessionId": self.session_id, "prompt": [{"type": "text", "text": text}]}),
            )
            .map_err(|_| AcpError::Closed)?;
            let deadline = Instant::now() + timeout;
            let mut reply = String::new();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    self.abort();
                    return Err(AcpError::Cancelled);
                }
                if Instant::now() >= deadline {
                    self.abort();
                    return Err(AcpError::Timeout);
                }
                let incoming = match self.conn.recv(Duration::from_millis(100)) {
                    Ok(incoming) => incoming,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err(AcpError::Closed),
                };
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
                            for out in merger.push(item) {
                                sink(out);
                            }
                        }
                    }
                    Incoming::Request { id, method, params } => {
                        if let Some(item) = merger.flush() {
                            sink(item);
                        }
                        self.answer(&id, &method, &params, gate, sink)?;
                    }
                    _ => {}
                }
            }
        })();
        if let Some(item) = merger.flush() {
            sink(item);
        }
        let stop = stop_reason.as_deref().unwrap_or(match &result {
            Err(AcpError::Cancelled) => "cancelled",
            Err(AcpError::Timeout) => "timeout",
            _ => "error",
        });
        sink(TraceItem::turn_end(
            stop,
            started.elapsed().as_millis() as u64,
        ));
        result
    }

    fn answer(
        &mut self,
        id: &Value,
        method: &str,
        params: &Value,
        gate: &Gate,
        sink: &mut dyn FnMut(TraceItem),
    ) -> Result<(), AcpError> {
        let sent = if method == "session/request_permission" {
            let request = PermissionRequest::from_params(params);
            let verdict = gate.judge(&request);
            let wanted = request
                .command
                .clone()
                .or_else(|| request.path.clone())
                // 解码失败仍保留请求原文；用于审计，绝不拿它重新做放行判定。
                .or_else(|| {
                    params["toolCall"]["rawInput"]
                        .get("command")
                        .map(Value::to_string)
                })
                .unwrap_or_default();
            let (title, result) = match &verdict {
                Verdict::Allow => ("权限请求：放行".to_string(), String::new()),
                Verdict::Reject(reason) => {
                    ("权限请求：拒绝".to_string(), format!("\n原因：{reason}"))
                }
            };
            sink(TraceItem::new(
                TraceKind::Permission,
                title,
                format!("{}：{wanted}{result}", request.tool),
            ));
            if let Verdict::Reject(reason) = &verdict {
                tracing::info!(session_id = %self.session_id, permission_id = %id,
                    %reason, wanted = %wanted, "大脑的权限请求被拒绝");
            }
            let outcome = match pick_option(params, &verdict) {
                Some(option_id) => json!({"outcome": "selected", "optionId": option_id}),
                None => json!({"outcome": "cancelled"}),
            };
            self.conn.respond(id, json!({"outcome": outcome}))
        } else {
            self.conn.respond_error(id, -32601, "unsupported")
        };
        sent.map_err(|_| AcpError::Closed)
    }

    fn abort(&mut self) {
        let _ = self
            .conn
            .notify("session/cancel", json!({"sessionId": self.session_id}));
        self.conn.kill();
    }
}

fn call(
    conn: &mut Connection,
    method: &str,
    params: Value,
    deadline: Instant,
) -> Result<Value, AcpError> {
    let id = conn
        .send_request(method, params)
        .map_err(|_| AcpError::Closed)?;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            return Err(AcpError::Timeout);
        }
        match conn.recv(wait.min(Duration::from_millis(200))) {
            Ok(Incoming::Response { id: rid, result }) if rid == id => {
                return result.map_err(|e| AcpError::Remote(e.to_string()));
            }
            Ok(Incoming::Request { id, .. }) => {
                let _ = conn.respond_error(&id, -32601, "unsupported");
            }
            Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(AcpError::Closed),
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
        env: env
            .into_iter()
            .chain(crate::backend::session_env(backend.kind))
            .collect(),
        meta: crate::backend::session_meta(backend.kind, None),
        mode: None,
        model: None,
        effort: None,
        handshake_timeout: Duration::from_secs(60),
    };
    Ok(AcpSession::start(&options)?.model_choices())
}
