//! 建会话的握手：initialize、session/new、模式和模型。连接可以是独占的，也可以是共用的。
use super::client::{AcpError, SessionOptions};
use super::wire::Rpc;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub(crate) fn handshake(
    conn: &mut dyn Rpc,
    options: &SessionOptions,
    cancel: &AtomicBool,
    staff: bool,
    initialize: bool,
    on_initialized: &mut dyn FnMut(),
) -> Result<(String, Value), AcpError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AcpError::Cancelled);
    }
    let deadline = Instant::now() + options.handshake_timeout;
    if initialize {
        let mut capabilities =
            json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false});
        if staff {
            capabilities["elicitation"] = json!({"form": {}});
        }
        call(
            conn,
            "initialize",
            json!({"protocolVersion": 1, "clientCapabilities": capabilities}),
            deadline,
            cancel,
        )?;
        // 连接只能初始化一次；之后 session/new 失败也不能再来一遍。
        on_initialized();
    }
    let mut params = json!({"cwd": options.cwd, "mcpServers": []});
    if let Some(meta) = &options.meta {
        params["_meta"] = meta.clone();
    }
    let created = call(conn, "session/new", params, deadline, cancel)?;
    let session_id = created["sessionId"]
        .as_str()
        .ok_or_else(|| AcpError::Remote("session/new 没有返回 sessionId".into()))?
        .to_string();
    if let Some(mode) = &options.mode {
        call(
            conn,
            "session/set_mode",
            json!({"sessionId": session_id, "modeId": mode}),
            deadline,
            cancel,
        )?;
    }
    let collaboration =
        (staff && options.kind == crate::backend::BackendKind::Codex).then(|| "plan".to_owned());
    let configs = [
        ("collaboration_mode", &collaboration),
        ("model", &options.model),
        ("reasoning_effort", &options.effort),
    ];
    for (config_id, value) in configs {
        let Some(value) = value else { continue };
        if let Err(error) = call(
            conn,
            "session/set_config_option",
            json!({"sessionId": session_id, "configId": config_id, "value": value}),
            deadline,
            cancel,
        ) {
            if matches!(error, AcpError::Cancelled) {
                return Err(error);
            }
            tracing::warn!(%error, config_id, value, "大脑会话设置失败");
        }
    }
    Ok((session_id, created))
}

fn call(
    conn: &mut dyn Rpc,
    method: &str,
    params: Value,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Value, AcpError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AcpError::Cancelled);
    }
    let id = conn
        .send_request(method, params)
        .map_err(|_| AcpError::Closed)?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AcpError::Cancelled);
        }
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            return Err(AcpError::Timeout);
        }
        match conn.recv(wait.min(std::time::Duration::from_millis(200))) {
            Ok(super::wire::Incoming::Response { id: rid, result }) if rid == id => {
                return result.map_err(|error| AcpError::Remote(error.to_string()));
            }
            Ok(super::wire::Incoming::Request { id, .. }) => {
                let _ = conn.respond_error(&id, -32601, "unsupported");
            }
            Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(AcpError::Closed),
        }
    }
}
