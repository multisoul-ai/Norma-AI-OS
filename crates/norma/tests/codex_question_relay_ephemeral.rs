#![cfg(unix)]
use norma::codex::relay::{CodexQuestionRelay, CodexRelayEvent};
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

fn fixture(name: &str) -> Value {
    serde_json::from_str(match name {
        "source" => include_str!("fixtures/questions/codex-source.json"),
        "request" => include_str!("fixtures/questions/codex-request.json"),
        _ => include_str!("fixtures/questions/codex-consumed.json"),
    })
    .unwrap()
}

/// 场景：真实Codex TUI会在同连接启动temporary-structured临时线程，不能替换卡片主会话。
/// 构造：原主thread与真实问答夹具，三轮分别在出题前、出题后、临时start未回包时提交。
/// 执行：原TUI启动ephemeral辅助线程，relay继续用首次主线程lease提交Green并读精确消费。
/// 预期：只发布一个主会话附着；临时请求完整透传且不改变raw设置，不撤销/阻塞主问题。
#[test]
fn ephemeral_thread_never_rebinds_or_holds_main_question() {
    for placement in ["before", "after", "unresolved"] {
        let dir = tempfile::Builder::new()
            .prefix("nqe-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            assert_eq!(read(&mut socket)["id"], "main", "先创建原主线程");
            send(
                &mut socket,
                json!({"id":"main","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            if placement != "before" {
                send(&mut socket, fixture("source"));
                send(&mut socket, fixture("request"));
            }
            let auxiliary = read(&mut socket);
            assert_eq!(
                auxiliary["params"]["ephemeral"], true,
                "原TUI明确辅助线程性质"
            );
            // Exact passthrough is checked after the main regression so the red identifies rebind.
            if placement != "unresolved" {
                send(
                    &mut socket,
                    json!({"id":"auxiliary","result":{"thread":{"id":"temporary-thread"}}}),
                );
            }
            if placement == "before" {
                send(&mut socket, fixture("source"));
                send(&mut socket, fixture("request"));
            }
            assert_eq!(
                read(&mut socket)["id"],
                "barrier",
                "TUI确认已读辅助响应与主问题"
            );
            send(&mut socket, json!({"id":"barrier","result":{}}));
            ready_tx.send(()).unwrap();
            assert_eq!(
                read(&mut socket),
                json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
                "只有原主请求收到一次答案"
            );
            send(&mut socket, fixture("consumed"));
            let _ = socket.read();
            assert_eq!(
                auxiliary["params"]["experimentalRawEvents"], false,
                "辅助线程不被擅加raw订阅"
            );
        });
        let tui = std::thread::spawn(move || {
            let mut socket = tungstenite::client("ws://localhost/", timed(native))
                .unwrap()
                .0;
            send(
                &mut socket,
                json!({"id":"main","method":"thread/start","params":{"ephemeral":false}}),
            );
            read(&mut socket);
            if placement != "before" {
                read(&mut socket);
                read(&mut socket);
            }
            send(
                &mut socket,
                json!({"id":"auxiliary","method":"thread/start","params":{"ephemeral":true,"experimentalRawEvents":false}}),
            );
            if placement != "unresolved" {
                read(&mut socket);
            }
            if placement == "before" {
                read(&mut socket);
                read(&mut socket);
            }
            send(
                &mut socket,
                json!({"id":"barrier","method":"thread/read","params":{"threadId":"codex-question-test-thread"}}),
            );
            assert_eq!(
                read(&mut socket)["id"],
                "barrier",
                "辅助工作不影响原TUI请求"
            );
            assert_eq!(
                read(&mut socket)["params"]["item"]["type"],
                "function_call_output",
                "主问题的消费回到原TUI"
            );
            let _ = socket.close(None);
        });
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut sessions = Vec::new();
        let mut question = None;
        loop {
            match relay.events().recv_timeout(Duration::from_millis(100)) {
                Ok(CodexRelayEvent::Session(session)) => {
                    sessions.push(session.thread_id().to_owned())
                }
                Ok(CodexRelayEvent::Question(change)) if change.state == QuestionState::Pending => {
                    question = Some(change.question)
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert_eq!(
            sessions,
            vec!["codex-question-test-thread"],
            "{placement}临时线程不能重绑定卡片"
        );
        let question = question.expect("主线程原题必须仍可回答");
        relay
            .answer(
                question,
                vec![QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }],
            )
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("辅助start不能撤销或阻塞原题");
        let mut consumed = false;
        while let Ok(event) = relay.events().recv_timeout(Duration::from_secs(5)) {
            if matches!(event,CodexRelayEvent::Question(change) if change.state == QuestionState::Answered)
            {
                consumed = true;
                break;
            }
        }
        assert!(consumed, "原主工具必须有精确消费，{placement}");
        tui.join().unwrap();
        server.join().unwrap();
    }
}
