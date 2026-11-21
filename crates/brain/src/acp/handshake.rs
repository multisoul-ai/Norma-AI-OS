//! 建会话的握手：initialize、session/new、模式和模型。连接可以是独占的，也可以是共用的。
use super::client::{AcpError, SessionOptions};
use super::wire::Rpc;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[cfg(test)]
#[path = "../../tests/unit/acp_handshake_tests.rs"]
mod tests;

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
    let codex_staff = staff && options.kind == crate::backend::BackendKind::Codex;
    let mut params = json!({"cwd": options.cwd, "mcpServers": []});
    if let Some(meta) = &options.meta {
        params["_meta"] = meta.clone();
    }
    if codex_staff {
        // Default 协作模式也要能通过原生问题卡请用户补充信息。
        params["_meta"]["norma"]["config"]["features"]["default_mode_request_user_input"] =
            json!(true);
    }
    let created = call(conn, "session/new", params, deadline, cancel)?;
    let session_id = created["sessionId"]
        .as_str()
        .ok_or_else(|| AcpError::Remote("session/new 没有返回 sessionId".into()))?
        .to_string();
    // Codex 的协作模式和沙箱是独立设置。幕僚需要执行 Norma 派发命令，文件写入交给 Worker。
    let mode = if codex_staff {
        Some("read-only")
    } else {
        options.mode.as_deref()
    };
    if let Some(mode) = mode {
        if let Err(error) = call(
            conn,
            "session/set_mode",
            json!({"sessionId": session_id, "modeId": mode}),
            deadline,
            cancel,
        ) {
            // 幕僚必须确认只读沙箱已设置；其他会话允许沿用适配器的默认模式。
            if codex_staff {
                // 共用连接会继续运行，不能留下已创建但不可用的会话。
                let _ = conn.send_request("session/close", json!({"sessionId": session_id}));
                return Err(error);
            }
            if matches!(error, AcpError::Cancelled) {
                return Err(error);
            }
            tracing::warn!(%error, mode, "大脑会话模式设置失败");
        }
    }
    let collaboration = codex_staff.then(|| "default".to_owned());
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
            if codex_staff
                && (config_id == "collaboration_mode" || matches!(error, AcpError::Cancelled))
            {
                let _ = conn.send_request("session/close", json!({"sessionId": session_id}));
                return Err(error);
            }
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
