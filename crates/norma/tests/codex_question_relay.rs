#![cfg(unix)]
use norma::codex::relay::{CodexQuestionRelay, CodexRelayEvent};
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：原生TUI自己创建会话，relay只补raw能力，回答同一连接上的原RPC。
/// 构造：本地Unix WebSocket两端模拟真实TUI与app-server，使用M0问答消息。
/// 执行：初始化、原thread/start、原问题、通过relay回复Green、工具消费后继续原TUI。
/// 预期：权限/cwd/原RPC不改变、无bootstrap/额外turn；原终端收到同消费且观察器确认Answered。
#[test]
fn native_relay_preserves_session_and_answers_original_rpc_without_extra_turn() {
    let dir = tempfile::Builder::new()
        .prefix("nqr-")
        .tempdir_in("/tmp")
        .unwrap();
    let upstream_path = dir.path().join("server.sock");
    let listener = UnixListener::bind(&upstream_path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        let init = read(&mut socket);
        assert_eq!(init["id"], "tui-init", "保留原TUI请求身份");
        send(&mut socket, json!({"id":"tui-init","result":{}}));
        assert_eq!(
            read(&mut socket)["method"],
            "initialized",
            "原初始化直接透传"
        );
        let start = read(&mut socket);
        assert_eq!(
            start["method"], "thread/start",
            "只能是原TUI创建，不能插bootstrap turn"
        );
        assert_eq!(
            start["params"]["experimentalRawEvents"], true,
            "为原thread/start开启来源通知"
        );
        assert_eq!(
            start["params"]["approvalPolicy"], "on-request",
            "不能改用户权限策略"
        );
        assert_eq!(
            start["params"]["cwd"], "/tmp/original-project",
            "不能换工作目录"
        );
        send(
            &mut socket,
            json!({"id":start["id"],"result":{"thread":{"id":"codex-question-test-thread"}}}),
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap(),
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!("fixtures/questions/codex-request.json")).unwrap(),
        );
        assert_eq!(
            read(&mut socket),
            json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
            "答案必须回原RPC，不能通过普通prompt转发"
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json")).unwrap(),
        );
        assert_eq!(
            read(&mut socket)["method"],
            "thread/read",
            "原TUI仍可继续发送后续请求"
        );
        send(&mut socket, json!({"id":"continued","result":{}}));
        let _ = socket.read();
    });
    let tui = std::thread::spawn(move || {
        let mut socket = tungstenite::client("ws://localhost/", timed(native))
            .unwrap()
            .0;
        send(
            &mut socket,
            json!({"id":"tui-init","method":"initialize","params":{"clientInfo":{"name":"codex_tui"}}}),
        );
        assert_eq!(read(&mut socket)["id"], "tui-init", "初始化响应保持原ID");
        send(&mut socket, json!({"method":"initialized"}));
        send(
            &mut socket,
            json!({"id":"start","method":"thread/start","params":{
                "cwd":"/tmp/original-project","approvalPolicy":"on-request","sandbox":"workspace-write"
            }}),
        );
        assert_eq!(
            read(&mut socket)["result"]["thread"]["id"],
            "codex-question-test-thread",
            "原TUI持有同thread"
        );
        assert_eq!(
            read(&mut socket)["method"],
            "rawResponseItem/completed",
            "来源原样透传"
        );
        assert_eq!(read(&mut socket)["id"], 0, "原TUI仍能显示原题");
        assert_eq!(
            read(&mut socket)["params"]["item"]["type"],
            "function_call_output",
            "原TUI收到消费结果"
        );
        send(
            &mut socket,
            json!({"id":"continued","method":"thread/read","params":{"threadId":"codex-question-test-thread"}}),
        );
        assert_eq!(read(&mut socket)["id"], "continued", "回答后原连接继续可用");
        socket.close(None).unwrap();
    });
    let relay = CodexQuestionRelay::connect(bridge, &upstream_path).unwrap();
    let question = loop {
        if let CodexRelayEvent::Question(change) =
            relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            assert_eq!(change.state, QuestionState::Pending, "核验后才发布可答题");
            break change.question;
        }
    };
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let ticket = prepared_ticket(&question, &answers);
    relay
        .answer_ticket(question, answers, "durable-delivery".into(), ticket.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert!(ticket.transport_sent(), "发送结果只在原RPC完整写出后确认");
    let mut confirmed = false;
    while !confirmed {
        if let CodexRelayEvent::Question(change) =
            relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            confirmed = change.state == QuestionState::Answered;
            if confirmed {
                assert_eq!(
                    change.delivery_id.as_deref(),
                    Some("durable-delivery"),
                    "消费绑定宿主提交身份"
                );
            }
        }
    }
    tui.join().unwrap();
    server.join().unwrap();
    assert!(confirmed, "同连接同工具结果确认消费");
}

/// 场景：人工先答或关闭问答观察器，原TUI必须继续；Norma不能再写旧答案。
/// 构造：同一pending题，通道控制人工发送Blue的时点；两轮分别保持/关闭观察器。
/// 执行：登记原题、放行人工回复、服务端确认Blue后尝试远端Green，继续thread/read。
/// 预期：服务端只收到Blue；远端被拒绝；关闭问答观察器不关闭TUI的WebSocket。
#[test]
fn native_answer_and_observer_disable_keep_the_original_tui_usable() {
    for disable in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("nqr-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("server.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (release, ready) = crossbeam_channel::bounded(1);
        let (received, confirmed) = crossbeam_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            assert_eq!(read(&mut socket)["method"], "thread/start", "原TUI发起会话");
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
            assert_eq!(
                read(&mut socket),
                json!({"id":0,"result":{"answers":{"label":{"answers":["Blue"]}}}}),
                "人工的原始Blue响应不可修改"
            );
            received.send(()).unwrap();
            let next = read(&mut socket);
            assert_eq!(
                next["method"], "thread/read",
                "没有多发Green且原TUI继续可用"
            );
            send(&mut socket, json!({"id":"continue","result":{}}));
            let _ = socket.read();
        });
        let (continue_tx, continue_rx) = crossbeam_channel::bounded(1);
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
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
            send(
                &mut socket,
                json!({"id":0,"result":{"answers":{"label":{"answers":["Blue"]}}}}),
            );
            continue_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            send(
                &mut socket,
                json!({"id":"continue","method":"thread/read","params":{"threadId":"codex-question-test-thread"}}),
            );
            assert_eq!(
                read(&mut socket)["id"],
                "continue",
                "原会话不被观察器停用影响"
            );
            socket.close(None).unwrap();
        });
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        let question = loop {
            if let CodexRelayEvent::Question(change) =
                relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
            {
                break change.question;
            }
        };
        if disable {
            relay.disable_questions();
        }
        release.send(()).unwrap();
        confirmed.recv_timeout(Duration::from_secs(5)).unwrap();
        let answers = vec![QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        }];
        let ticket = prepared_ticket(&question, &answers);
        let response =
            relay.answer_ticket(question, answers, "durable-delivery".into(), ticket.clone());
        assert!(
            response.is_err()
                || response
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .is_err(),
            "人工已答或观察器停用后不能发送旧建议，disable={disable}"
        );
        assert_eq!(
            ticket.native_send_status(),
            norma::agent_hook::NativeSendStatus::Unsent,
            "发送资格拒绝发生在任何IO之前，不能把未发送标为Unknown"
        );
        ticket.cancel();
        assert_eq!(
            ticket.status(std::time::Instant::now()),
            norma::agent_hook::QuestionHookStatus::Cancelled,
            "人工先答后的旧ticket仍可撤销"
        );
        assert!(relay.is_running(), "只停问答接管，原连接继续");
        continue_tx.send(()).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
    }
}

/// 场景：TUI重新附着同thread、同RPC时，旧建议不能占用新一代可答请求。
/// 构造：同Unix连接先start后resume，服务端两次重放相同来源和pending；仅新句柄获准。
/// 执行：保留旧句柄、完成resume、分别提交旧/新句柄；检查resume参数与服务端收到的响应。
/// 预期：旧句柄被拒绝、新句柄成功；resume不得擅加不支持的raw字段，也不插入新turn。
#[test]
fn resumed_same_rpc_requires_the_new_attachment_identity() {
    let dir = tempfile::Builder::new()
        .prefix("nqr-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("server.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let (resume_tx, resume_rx) = crossbeam_channel::bounded(1);
    let server = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        for expected in ["thread/start", "thread/resume"] {
            let input = read(&mut socket);
            assert_eq!(input["method"], expected, "只有原TUI自己的会话请求");
            if expected == "thread/resume" {
                assert!(
                    input["params"].get("experimentalRawEvents").is_none(),
                    "resume没有raw能力字段，不能伪造"
                );
            }
            send(
                &mut socket,
                json!({"id":input["id"],"result":{"thread":{"id":"codex-question-test-thread"}}}),
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
        }
        assert_eq!(
            read(&mut socket),
            json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
            "仅新句柄发送一次Green"
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json")).unwrap(),
        );
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
        resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        send(
            &mut socket,
            json!({"id":"resume","method":"thread/resume","params":{"threadId":"codex-question-test-thread"}}),
        );
        for _ in 0..3 {
            read(&mut socket);
        }
        read(&mut socket);
        socket.close(None).unwrap();
    });
    let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
    let pending = || loop {
        if let CodexRelayEvent::Question(change) =
            relay.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            if change.state == QuestionState::Pending {
                break change.question;
            }
        }
    };
    let old = pending();
    resume_tx.send(()).unwrap();
    let current = pending();
    let answers = || {
        vec![QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        }]
    };
    assert!(
        relay
            .answer(old, answers())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err(),
        "新一代连接登记即使RPC内容相同也不能被旧句柄回答"
    );
    relay
        .answer(current, answers())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    tui.join().unwrap();
    server.join().unwrap();
}

fn prepared_ticket(
    question: &norma::codex::relay::CodexQuestionLease,
    answers: &[QuestionAnswer],
) -> norma::agent_hook::HookQuestion {
    let adapter = question.question();
    let ticket = norma::agent_hook::HookQuestion::new(norma::agent_hook::HookQuestionRequest {
        run_id:"owned-run".into(), event:norma::agent_hook::AgentHookEvent {
            control_run_id:Some("owned-run".into()),node_id:7,profile:norma::protocol::AgentProfile::Codex,
            provider_event:"NormaQuestion".into(), session_id:Some(adapter.thread_id().into()),
            turn_id:Some(adapter.turn_id().into()), tool_name:Some("request_user_input".into()),message:None,
            raw_json:json!({"connection_id":question.connection_id(),"request":adapter.original_request(),"source":adapter.original_source()}),
        },
    }, std::time::Instant::now()).unwrap();
    ticket
        .submit_delivery(answers, "durable-delivery", std::time::Instant::now())
        .unwrap();
    ticket.take_response(std::time::Instant::now()).unwrap();
    ticket
}
