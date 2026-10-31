#![cfg(unix)]
use norma::codex::relay::{CodexQuestionRelay, CodexRelayEvent};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：实验raw协议只对实测版本开放，未知版本仍保持原TUI连接可用。
/// 构造：真实Unix socket，initialize分别返回0.162.1、0.166.0、未实测版本和缺失版本。
/// 执行：原TUI自行initialize/start；核对只在已核验版本注入raw，再由原TUI回复原题。
/// 预期：已核验版本有原题事件，未知版本无代答事件；全部透传人工答案，无额外请求。
#[test]
fn verified_relay_gates_experimental_answers_on_original_initialize() {
    for version in [
        Some("codex-tui/0.162.1 (test)"),
        Some("codex-tui/0.166.0 (test)"),
        Some("codex-tui/0.163.0 (test)"),
        None,
    ] {
        let supported = matches!(
            version,
            Some("codex-tui/0.162.1 (test)" | "codex-tui/0.166.0 (test)")
        );
        let dir = tempfile::Builder::new()
            .prefix("nqv-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            assert_eq!(
                read(&mut socket)["method"],
                "initialize",
                "必须透传原TUI初始化"
            );
            send(
                &mut socket,
                json!({"id":"init","result":{"userAgent":version}}),
            );
            let start = read(&mut socket);
            assert_eq!(start["method"], "thread/start", "不可插入其它RPC");
            assert_eq!(
                start["params"]["experimentalRawEvents"]
                    .as_bool()
                    .unwrap_or(false),
                supported,
                "仅实测版本注入实验字段"
            );
            send(
                &mut socket,
                json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            send(
                &mut socket,
                serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap(),
            );
            send(
                &mut socket,
                serde_json::from_str(include_str!("fixtures/questions/codex-request.json"))
                    .unwrap(),
            );
            assert_eq!(read(&mut socket)["id"], 0, "原TUI仍可回答原问题");
            send(
                &mut socket,
                serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json"))
                    .unwrap(),
            );
            let _ = socket.read();
        });
        let (finish, wait) = crossbeam_channel::bounded(1);
        let tui = std::thread::spawn(move || {
            let mut socket = tungstenite::client("ws://localhost/", timed(native))
                .unwrap()
                .0;
            send(
                &mut socket,
                json!({"id":"init","method":"initialize","params":{}}),
            );
            read(&mut socket);
            send(
                &mut socket,
                json!({"id":"start","method":"thread/start","params":{}}),
            );
            for _ in 0..3 {
                read(&mut socket);
            }
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            send(
                &mut socket,
                json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
            );
            assert_eq!(
                read(&mut socket)["params"]["item"]["type"],
                "function_call_output",
                "未知版本也保留原生功能"
            );
            let _ = socket.close(None);
        });
        let relay = CodexQuestionRelay::connect_verified(bridge, &path, false).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let questions = relay
            .events()
            .try_iter()
            .filter(|e| matches!(e, CodexRelayEvent::Question(_)))
            .count();
        finish.send(()).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
        assert_eq!(questions > 0, supported, "未核验版本绝不开代答");
    }
}
