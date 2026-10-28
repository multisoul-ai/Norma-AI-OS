#![cfg(unix)]
use norma::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest, NativeSendStatus};
use norma::codex::relay::{CodexQuestionRelay, CodexRelayEvent};
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionState};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
use tungstenite::Message;
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：宿主大答案部分写出时遇到Ping或连接关闭，ticket必须反映真正的传输阶段。
/// 构造：16道自定义题×32768字节=512KiB答案；server先不读，强制Unix socket背压。
/// 执行：等到Writing，分别Drop relay、关闭TUI，或发送Ping后放开读取并立即回消费结果。
/// 预期：断连一律Unknown且不能重发；成功flush后才发完成回执，消费携带原delivery身份。
#[test]
fn partial_answer_ping_and_disconnect_keep_transport_ownership_accurate() {
    for mode in ["drop", "tui-close", "complete"] {
        let dir = tempfile::Builder::new()
            .prefix("nqp-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (release, gate) = crossbeam_channel::bounded::<bool>(1);
        let (finish, finished) = crossbeam_channel::bounded::<bool>(1);
        let (request, source) = large_question();
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            read(&mut socket);
            send(
                &mut socket,
                json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            send(&mut socket, source);
            send(&mut socket, request);
            if !gate.recv_timeout(Duration::from_secs(10)).unwrap() {
                return;
            }
            socket.send(Message::Ping(vec![7].into())).unwrap();
            let answer = read(&mut socket);
            assert_eq!(answer["id"], 0, "完整答案只回原RPC");
            assert_eq!(
                answer["result"]["answers"].as_object().unwrap().len(),
                16,
                "不得截断多题答案"
            );
            let mut consumed: Value =
                serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json"))
                    .unwrap();
            consumed["params"]["item"]["output"] = answer["result"].to_string().into();
            send(&mut socket, consumed);
            assert_eq!(
                read(&mut socket)["method"],
                "continued",
                "确认原TUI收完消费后再关闭测试服务端"
            );
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
            if finished.recv_timeout(Duration::from_secs(10)).unwrap() {
                assert_eq!(
                    read(&mut socket)["params"]["item"]["type"],
                    "function_call_output",
                    "原TUI收到同一消费"
                );
                send(&mut socket, json!({"method":"continued"}));
            }
            let _ = socket.close(None);
        });
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        let lease = loop {
            if let CodexRelayEvent::Question(change) =
                relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
            {
                break change.question;
            }
        };
        let original = lease.question();
        let ticket = HookQuestion::new(HookQuestionRequest {
            run_id:"run".into(),event:AgentHookEvent {
                control_run_id:Some("run".into()),node_id:7,profile:AgentProfile::Codex,
                provider_event:"NormaQuestion".into(),session_id:Some(original.thread_id().into()),
                turn_id:Some(original.turn_id().into()),tool_name:Some("request_user_input".into()),message:None,
                raw_json:json!({"connection_id":lease.connection_id(),"request":original.original_request(),"source":original.original_source()}),
            }
        },Instant::now()).unwrap();
        let answers = (0..16)
            .map(|index| QuestionAnswer {
                key: format!("question-{index}"),
                selected: vec![],
                text: Some("x".repeat(32768)),
            })
            .collect::<Vec<_>>();
        ticket
            .submit_delivery(&answers, "delivery", Instant::now())
            .unwrap();
        ticket.take_response(Instant::now()).unwrap();
        let sent = relay
            .answer_ticket(lease, answers, "delivery".into(), ticket.clone())
            .unwrap();
        wait_for(|| ticket.native_send_status() != NativeSendStatus::Unsent);
        assert_eq!(
            ticket.native_send_status(),
            NativeSendStatus::Writing,
            "{mode}必须实际触发部分写出"
        );
        assert!(sent.try_recv().is_err(), "部分写出不能提前报告完整发送");
        if mode == "complete" {
            release.send(true).unwrap();
            sent.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            assert_eq!(
                ticket.native_send_status(),
                NativeSendStatus::Sent,
                "读路径不能绕过ticket的发送回执"
            );
            loop {
                if let CodexRelayEvent::Question(change) =
                    relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
                {
                    if change.state == QuestionState::Answered {
                        assert_eq!(
                            change.delivery_id.as_deref(),
                            Some("delivery"),
                            "工具消费归属本次提交"
                        );
                        assert!(ticket.transport_sent(), "消费发布前已完成传输状态同步");
                        break;
                    }
                }
            }
            finish.send(true).unwrap();
            tui.join().unwrap();
            server.join().unwrap();
        } else {
            if mode == "drop" {
                drop(relay);
            } else {
                finish.send(false).unwrap();
            }
            wait_for(|| ticket.native_send_status() != NativeSendStatus::Writing);
            let state = ticket.native_send_status();
            release.send(false).unwrap();
            if mode == "drop" {
                finish.send(false).unwrap();
            }
            tui.join().unwrap();
            server.join().unwrap();
            assert_eq!(
                state,
                NativeSendStatus::Unknown,
                "{mode}必须统一终结可能部分写出的ticket"
            );
            assert!(
                sent.recv_timeout(Duration::from_secs(1)).unwrap().is_err(),
                "等待者收到明确的不确定发送错误"
            );
            let mut retried = false;
            assert!(
                ticket
                    .with_native_send("delivery", Instant::now(), || {
                        retried = true;
                        Ok(true)
                    })
                    .is_err(),
                "断连后不得重发"
            );
            assert!(!retried, "未知结果必须阻止第二次IO");
        }
    }
}

fn wait_for(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn large_question() -> (Value, Value) {
    let mut request: Value =
        serde_json::from_str(include_str!("fixtures/questions/codex-request.json")).unwrap();
    let mut source: Value =
        serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap();
    let questions: Vec<_> = (0..16)
        .map(|index| {
            let mut question = request["params"]["questions"][0].clone();
            question["id"] = format!("item-{index}").into();
            question
        })
        .collect();
    request["params"]["questions"] = json!(questions);
    source["params"]["item"]["arguments"] = json!({"questions":questions}).to_string().into();
    (request, source)
}
