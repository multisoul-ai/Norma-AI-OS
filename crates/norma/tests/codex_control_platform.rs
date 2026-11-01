#![cfg(any(unix, windows))]
use norma::codex::native_questions::{CodexNativeEvent, CodexQuestionHost};
use norma::codex::relay::CodexQuestionRelay;
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::json;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};
mod support {
    use super::UnixStream;
    use serde_json::Value;
    use std::time::Duration;
    use tungstenite::{Message, WebSocket};
    pub fn read(socket: &mut WebSocket<UnixStream>) -> Value {
        loop {
            if let Message::Text(text) = socket.read().expect("应收到协议帧") {
                return serde_json::from_str(&text).expect("应为 JSON");
            }
        }
    }
    pub fn send(socket: &mut WebSocket<UnixStream>, value: Value) {
        socket
            .send(Message::Text(value.to_string().into()))
            .expect("应写出协议帧");
    }
    pub fn timed(stream: UnixStream) -> UnixStream {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
    }
}
use support::{read, send, timed};

/// 场景：同进程宿主从原relay取得ticket，采用后自动回答原问题并回传消费身份。
/// 构造：同Unix连接的原TUI与server，真实M0来源/pending/result；明确node和进程run。
/// 执行：接收独立Attached、Question，宿主提交Green，桥自动发送并发布消费事件。
/// 预期：连接由附着事件先建立，问题不自行改连接；无HTTP许可窗口，原TUI保持可用。
#[test]
fn platform_host_preserves_attachment_answer_cancel_and_peer_shutdown() {
    for cancel in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("nqh-")
            .tempdir_in(if cfg!(windows) {
                std::env::temp_dir()
            } else {
                "/tmp".into()
            })
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
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.is_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !host.is_running(),
            "原生连接关闭后宿主必须结束，cancel={cancel}"
        );
        assert!(
            !host.is_available(),
            "关闭后不再接受新答案，cancel={cancel}"
        );
    }
}

/// 场景：控制 RPC 返回拒绝后仍可查询，但关闭连接时在途请求不能假报成功或重放。
/// 构造：真实本地 AF_UNIX WebSocket，首次 RPC 返回带码错误，第二条只确认收到而不回复。
/// 执行：核对拒绝，发送第二条，等服务端确认后 drop 客户端，再读取在途结果与服务端 EOF。
/// 预期：无效 ID 在 IO 前被拒绝；原始错误保留；关闭在 5 秒内终结双方且无第三条重复请求。
#[test]
fn platform_control_errors_and_drop_resolve_pending_requests_without_replay() {
    let dir = tempfile::Builder::new()
        .prefix("nce-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (received, ready) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        let init = read(&mut socket);
        assert_eq!(init["method"], "initialize", "先完成同连接握手");
        send(&mut socket, json!({"id":init["id"],"result":{}}));
        assert_eq!(
            read(&mut socket)["method"],
            "initialized",
            "初始化确认不可遗漏"
        );
        let denied = read(&mut socket);
        assert_eq!(
            denied["method"], "thread/read",
            "无效响应 ID 不得生成额外帧"
        );
        send(
            &mut socket,
            json!({"id":denied["id"],"error":{"code":-32001,"message":"fixture-denied"}}),
        );
        let pending = read(&mut socket);
        assert_eq!(
            pending["method"], "turn/interrupt",
            "错误后连接仍可处理下一请求"
        );
        assert_ne!(pending["id"], denied["id"], "请求 ID 不能复用");
        received.send(()).unwrap();
        assert!(
            socket.read().is_err(),
            "drop 必须关闭对端，不能重放在途请求"
        );
    });
    let client = norma::codex::CodexConnection::connect(&path).unwrap();
    assert!(
        client.respond(json!(true), json!({})).is_err(),
        "无效请求 ID 在发送前拒绝"
    );
    let denied = client
        .request("thread/read", json!({"threadId":"owned"}))
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(
        denied.contains("fixture-denied") && denied.contains("-32001"),
        "保留原服务端诊断：{denied}"
    );
    assert!(client.is_connected(), "单个 RPC 拒绝不能伪装成整个连接断开");
    let pending = client
        .request(
            "turn/interrupt",
            json!({"threadId":"owned","turnId":"turn"}),
        )
        .unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(client);
    let failure = pending
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(
        failure.contains("outcome may be unknown") && failure.contains("do not replay"),
        "断连结果不能变成成功或允许重试：{failure}"
    );
    server.join().unwrap();
}
