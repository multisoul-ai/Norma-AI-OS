use super::handshake;
use crate::acp::client::SessionOptions;
use crate::acp::wire::{Incoming, Rpc};
use crate::backend::{session_mode, BackendKind};
use serde_json::{json, Value};
use std::io;
use std::sync::{atomic::AtomicBool, mpsc::RecvTimeoutError};
use std::time::Duration;

struct CodexRpc {
    sandbox: Value,
    collaboration: Value,
    reject_sandbox: bool,
    session_params: Value,
    closed: bool,
    next: Option<Incoming>,
}

impl CodexRpc {
    fn new() -> Self {
        Self {
            sandbox: json!("agent"),
            collaboration: json!("default"),
            reject_sandbox: false,
            session_params: Value::Null,
            closed: false,
            next: None,
        }
    }
}

impl Rpc for CodexRpc {
    fn send_request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        let result = match method {
            "initialize" => Ok(json!({})),
            "session/new" => {
                self.session_params = params;
                Ok(json!({"sessionId": "staff-session"}))
            }
            "session/close" => {
                self.closed = params["sessionId"] == "staff-session";
                Ok(json!({}))
            }
            "session/set_mode" if self.reject_sandbox => Err(json!({"code": -32602})),
            "session/set_mode" => {
                self.sandbox = params["modeId"].clone();
                Ok(json!({}))
            }
            "session/set_config_option" if params["configId"] == "collaboration_mode" => {
                self.collaboration = params["value"].clone();
                Ok(json!({}))
            }
            _ => panic!("unexpected handshake request {method}"),
        };
        self.next = Some(Incoming::Response { id: 1, result });
        Ok(1)
    }

    fn respond(&mut self, _: &Value, _: Value) -> io::Result<()> {
        unreachable!("the fixture does not issue requests")
    }

    fn respond_error(&mut self, _: &Value, _: i64, _: &str) -> io::Result<()> {
        unreachable!("the fixture does not issue requests")
    }

    fn recv(&mut self, _: Duration) -> Result<Incoming, RecvTimeoutError> {
        self.next.take().ok_or(RecvTimeoutError::Disconnected)
    }
}

fn options() -> SessionOptions {
    SessionOptions {
        kind: BackendKind::Codex,
        program: "codex-acp".into(),
        args: vec![],
        cwd: "/tmp".into(),
        env: vec![],
        meta: None,
        mode: session_mode(BackendKind::Codex).map(str::to_owned),
        model: None,
        effort: None,
        handshake_timeout: Duration::from_secs(1),
        staff_shell: Some("/tmp/norma".into()),
    }
}

/// 场景：Codex 幕僚需实际执行 Norma 派发命令，项目写入仍交给 Worker。
/// 数据构造：模拟 ACP 2.2.2 分别应用沙箱模式与协作模式的请求。
/// 执行过程：以同一后端分别完成幕僚握手与非幕僚握手。
/// 预期结果：幕僚使用默认协作模式和只读沙箱，非幕僚保持原有工作区写入模式。
#[test]
fn codex_staff_uses_default_collaboration_with_read_only_sandbox() {
    let mut staff = CodexRpc::new();
    let mut other = CodexRpc::new();
    let cancel = AtomicBool::new(false);
    handshake(&mut staff, &options(), &cancel, true, true, &mut || {}).expect("staff handshake");
    handshake(&mut other, &options(), &cancel, false, true, &mut || {})
        .expect("non-staff handshake");
    assert_eq!(staff.collaboration, "default", "幕僚派发不能受计划模式阻断");
    assert_eq!(staff.sandbox, "read-only", "幕僚不能直接写项目文件");
    let question_feature = |rpc: &CodexRpc| {
        rpc.session_params["_meta"]["norma"]["config"]["features"]
            ["default_mode_request_user_input"]
            .as_bool()
    };
    assert_eq!(question_feature(&staff), Some(true), "幕僚保留原生提问工具");
    assert_eq!(question_feature(&other), None, "非幕僚不覆盖提问配置");
    assert_eq!(other.collaboration, "default", "非幕僚的协作模式维持不变");
    assert_eq!(other.sandbox, "workspace-write", "非幕僚仍使用既有沙箱");
}

/// 场景：适配器拒绝设置幕僚的只读沙箱。
/// 数据构造：模拟 `session/set_mode` 返回 Invalid params。
/// 执行过程：尝试为 Codex 幕僚建会话。
/// 预期结果：握手失败，原先可写的会话不能接到任务。
#[test]
fn codex_staff_does_not_continue_without_read_only_sandbox() {
    let mut staff = CodexRpc::new();
    staff.reject_sandbox = true;
    let cancel = AtomicBool::new(false);
    let result = handshake(&mut staff, &options(), &cancel, true, true, &mut || {});
    assert!(
        result.is_err(),
        "只读沙箱未生效，不能继续向 Codex 幕僚发任务"
    );
    assert!(staff.closed, "应关闭已创建的会话，避免在共用连接上残留");
}
