#![cfg(unix)]
use norma::codex::native_questions::{CodexNativeEvent, CodexQuestionHost};
use norma::codex::relay::CodexQuestionRelay;
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：同进程宿主从原relay取得ticket，采用后自动回答原问题并回传消费身份。
/// 构造：同Unix连接的原TUI与server，真实M0来源/pending/result；明确node和进程run。
/// 执行：接收独立Attached、Question，宿主提交Green，桥自动发送并发布消费事件。
/// 预期：连接由附着事件先建立，问题不自行改连接；无HTTP许可窗口，原TUI保持可用。
#[test]
fn same_process_host_carries_owned_attachment_ticket_and_consumption() {
    for cancel in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("nqh-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (continue_tx, continue_rx) = crossbeam_channel::bounded::<bool>(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            assert_eq!(
                read(&mut socket)["method"],
                "thread/start",
                "TUI自己创建会话"
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
            if !cancel {
                assert_eq!(
                    read(&mut socket),
                    json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
                    "桥只回原RPC"
                );
                send(
                    &mut socket,
                    serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json"))
                        .unwrap(),
                );
            }
            assert_eq!(
                read(&mut socket)["method"],
                "continued",
                "取消时不应有任何答案RPC，原连接仍继续"
            );
            send(&mut socket, json!({"id":"continue","result":{}}));
            let _ = socket.read();
        });
        let tui = std::thread::spawn(move || {
            let mut socket = tungstenite::client("ws://localhost/", timed(native))
                .unwrap()
                .0;
            send(
                &mut socket,
                json!({"id":"start","method":"thread/start","params":{}}),
            );
            for _ in 0..3 {
                read(&mut socket);
            }
            if continue_rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                assert_eq!(
                    read(&mut socket)["params"]["item"]["type"],
                    "function_call_output",
                    "TUI收到原问题消费"
                );
            }
            send(&mut socket, json!({"id":"continue","method":"continued"}));
            assert_eq!(read(&mut socket)["id"], "continue", "原TUI可继续");
            let _ = socket.close(None);
        });
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        let host = CodexQuestionHost::start(relay, 7, "owned-run".into()).unwrap();
        let attached = match host.events().recv_timeout(Duration::from_secs(5)).unwrap() {
            CodexNativeEvent::Attached(attachment) => attachment,
            other => panic!("题目前必须先有受信任附着事件：{other:?}"),
        };
        assert_eq!(
            attached.thread_id(),
            "codex-question-test-thread",
            "绑定原TUI线程"
        );
        match host.events().recv_timeout(Duration::from_secs(5)).unwrap() {
            CodexNativeEvent::SessionUsed(used) => assert_eq!(
                used.thread_id(),
                attached.thread_id(),
                "真实问题使原线程可持久化"
            ),
            other => panic!("原题出现前应通知可信会话已产生历史：{other:?}"),
        }
        let ticket = match host.events().recv_timeout(Duration::from_secs(5)).unwrap() {
            CodexNativeEvent::Question(ticket) => ticket,
            other => panic!("应取得原题ticket：{other:?}"),
        };
        assert_eq!(
            ticket.request().run_id,
            "owned-run",
            "ticket沿用拥有者进程身份"
        );
        assert_eq!(
            ticket.adapter().connection_id("owned-run"),
            attached.connection_id(),
            "同一附着生成题目"
        );
        if cancel {
            assert!(ticket.try_cancel(), "宿主未提交时可直接接管原TUI");
            assert!(
                !ticket
                    .submit_delivery(&[], "delivery", Instant::now())
                    .unwrap_or(false),
                "取消后不得交答案"
            );
        } else {
            ticket
                .submit_delivery(
                    &[QuestionAnswer {
                        key: "question-0".into(),
                        selected: vec!["option-1".into()],
                        text: None,
                    }],
                    "delivery",
                    Instant::now(),
                )
                .unwrap();
            loop {
                if let CodexNativeEvent::Changed(change) =
                    host.events().recv_timeout(Duration::from_secs(5)).unwrap()
                {
                    if change.state == QuestionState::Answered {
                        assert_eq!(
                            change.delivery_id.as_deref(),
                            Some("delivery"),
                            "消费保留宿主提交身份"
                        );
                        assert!(ticket.transport_sent(), "工具消费前已完整写出");
                        break;
                    }
                }
            }
        }
        continue_tx.send(!cancel).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
    }
}
