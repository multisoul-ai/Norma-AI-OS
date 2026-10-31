#![cfg(unix)]
use norma::codex::relay::CodexQuestionRelay;
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 原TUI依次发出的会话请求：带本地sandbox的新建、带profile的恢复、fork与临时辅助线程。
fn session_requests() -> Vec<Value> {
    vec![
        json!({"id":"start","method":"thread/start","params":{"approvalPolicy":null,"sandbox":"workspace-write","permissions":null}}),
        json!({"id":"resume","method":"thread/resume","params":{"threadId":"saved","permissions":"workspace"}}),
        json!({"id":"fork","method":"thread/fork","params":{"threadId":"saved"}}),
        json!({"id":"helper","method":"thread/start","params":{"ephemeral":true,"sandbox":"read-only"}}),
    ]
}

/// 经relay转发会话请求，返回server实际收到的请求；`initialize_agent`模拟原生initialize回包。
fn forwarded(full_access: bool, initialize_agent: &str) -> Vec<Value> {
    let dir = tempfile::Builder::new()
        .prefix("nqp-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let agent = initialize_agent.to_owned();
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        assert_eq!(read(&mut socket)["method"], "initialize");
        send(
            &mut socket,
            json!({"id":"init","result":{"userAgent":agent}}),
        );
        let mut received = Vec::new();
        for request in session_requests() {
            let value = read(&mut socket);
            assert_eq!(value["id"], request["id"], "请求按原顺序且不新增");
            send(
                &mut socket,
                json!({"id":value["id"],"result":{"thread":{"id":value["params"]["threadId"].as_str().unwrap_or("new")}}}),
            );
            received.push(value);
        }
        received
    });
    let tui = std::thread::spawn(move || {
        let mut socket = tungstenite::client("ws://localhost/", timed(native))
            .unwrap()
            .0;
        send(
            &mut socket,
            json!({"id":"init","method":"initialize","params":{}}),
        );
        read(&mut socket);
        for request in session_requests() {
            send(&mut socket, request);
            read(&mut socket);
        }
        let _ = socket.close(None);
    });
    let relay = CodexQuestionRelay::connect_verified(bridge, &path, full_access).unwrap();
    tui.join().unwrap();
    let received = server.join().unwrap();
    drop(relay);
    received
}

/// 场景：Codex远程TUI带权限覆盖时拒绝/resume与fork，YOLO改由relay随会话请求携带。
/// 构造：已核验与未知版本initialize（后者令问题观察停用），原TUI发start/resume/fork/临时start。
/// 执行：全权限relay转发，读取server实际收到的参数。
/// 预期：四类请求都带never+danger-full-access且去掉冲突profile；观察停用不影响权限。
#[test]
fn full_access_relay_carries_yolo_on_every_session_request() {
    for agent in ["codex-tui/0.166.0 (test)", "codex-tui/9.9.9 (test)"] {
        for request in forwarded(true, agent) {
            let params = &request["params"];
            assert_eq!(params["approvalPolicy"], "never", "{agent} {request}");
            assert_eq!(params["sandbox"], "danger-full-access", "{agent} {request}");
            assert!(
                params["permissions"].is_null(),
                "profile与sandbox互斥：{request}"
            );
        }
    }
}

/// 场景：非YOLO启动不能被relay升级权限，恢复的会话沿用其保存的权限。
/// 构造：未开启全权限的已核验relay，同样四类会话请求。
/// 执行：转发并读取server实际收到的参数。
/// 预期：审批、沙箱与profile与原TUI发出的一致（仅主线程按原逻辑追加raw事件）。
#[test]
fn default_relay_never_escalates_session_permissions() {
    let received = forwarded(false, "codex-tui/0.166.0 (test)");
    for (request, original) in received.iter().zip(session_requests()) {
        for key in ["approvalPolicy", "sandbox", "permissions"] {
            assert_eq!(
                request["params"][key], original["params"][key],
                "{key}: {request}"
            );
        }
    }
}
