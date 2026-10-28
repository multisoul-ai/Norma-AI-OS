#![cfg(unix)]
use norma::codex::{native_questions::CodexQuestionHost, relay::CodexQuestionRelay};
use runtime::{AgentHookRuntimeConfig, Runtime};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：恢复只给pending或来源是connector时，原交互仍占用输入但无代答权限。
/// 构造：真实Unix relay，原TUI自行resume；两分支分别缺来源、给connector来源。
/// 执行：登记宿主、等待题目透传，再检查任务阻塞；人工完成原工具并重放旧pending。
/// 预期：无可答题/无自动RPC，精确原工具结束后解除阻塞，迟到重放不重新占用输入。
#[test]
fn unverified_native_request_blocks_tasks_without_becoming_answerable() {
    for connector in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("nqb-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (ready, ready_rx) = crossbeam_channel::bounded(1);
        let (finish, finish_rx) = crossbeam_channel::bounded(1);
        let (switch, switch_rx) = crossbeam_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            assert_eq!(
                read(&mut socket)["method"],
                "thread/resume",
                "只恢复原TUI指定会话"
            );
            send(
                &mut socket,
                json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            if connector {
                let mut source: Value = serde_json::from_str(include_str!(
                    "../../norma/tests/fixtures/questions/codex-source.json"
                ))
                .unwrap();
                source["params"]["item"]["name"] = "mcp__connector__approval".into();
                send(&mut socket, source);
            }
            let mut request: Value = serde_json::from_str(include_str!(
                "../../norma/tests/fixtures/questions/codex-request.json"
            ))
            .unwrap();
            send(&mut socket, request.clone());
            assert_eq!(
                read(&mut socket)["method"],
                "thread/start",
                "原TUI切换到新会话"
            );
            send(
                &mut socket,
                json!({"id":"next","result":{"thread":{"id":"next-thread"}}}),
            );
            request["params"]["threadId"] = "next-thread".into();
            send(&mut socket, request.clone());
            assert_eq!(read(&mut socket)["id"], 0, "人工仅回答原RPC");
            let mut consumed: Value = serde_json::from_str(include_str!(
                "../../norma/tests/fixtures/questions/codex-consumed.json"
            ))
            .unwrap();
            consumed["params"]["threadId"] = "next-thread".into();
            send(&mut socket, consumed);
            send(&mut socket, request);
            assert_eq!(
                read(&mut socket)["method"],
                "continued",
                "没有代答或额外turn"
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
                json!({"id":"start","method":"thread/resume","params":{"threadId":"codex-question-test-thread"}}),
            );
            for _ in 0..(if connector { 3 } else { 2 }) {
                read(&mut socket);
            }
            ready.send(()).unwrap();
            switch_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            send(
                &mut socket,
                json!({"id":"next","method":"thread/start","params":{}}),
            );
            for _ in 0..2 {
                read(&mut socket);
            }
            ready.send(()).unwrap();
            finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            send(
                &mut socket,
                json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
            );
            for _ in 0..2 {
                read(&mut socket);
            }
            send(&mut socket, json!({"id":"continued","method":"continued"}));
            assert_eq!(read(&mut socket)["id"], "continued", "原TUI继续可用");
            let _ = socket.close(None);
        });
        let mut runtime = Runtime::new_for_workspace("/tmp/codex-blocker-test");
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "r".into(),
            norma_socket: "/tmp/n.sock".into(),
        }));
        runtime.set_pty_factory(Box::new(|_, _, _, sender| {
            Ok(Box::new(terminal::TestPtyBackend::echo(sender)))
        }));
        let node = runtime
            .spawn_agent(
                canvas::AgentProfile::Codex,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .unwrap();
        let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
        let relay = CodexQuestionRelay::connect(bridge, &path).unwrap();
        runtime
            .adopt_codex_question_host(CodexQuestionHost::start(relay, node, process).unwrap())
            .unwrap();
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let blocked_before_drain = runtime.has_pending_question(node);
        let task_before_drain =
            runtime.send_agent_task_text(node, "must not reach original question");
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        let blocked = runtime.has_pending_question(node);
        let count = runtime.questions().len();
        switch.send(()).unwrap();
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.agent_session_id(node).as_deref() != Some("next-thread")
            && Instant::now() < deadline
        {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        let switched_state = runtime.agent_task_state(node);
        let saved_sessions = runtime.take_unprobed_session_titles();
        finish.send(()).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
        assert!(
            blocked_before_drain,
            "原请求已到TUI，即使前台尚未drain也必须阻止任务提交"
        );
        assert!(task_before_drain.is_err(), "任务提交必须核对宿主实时输入锁");
        assert!(
            blocked,
            "来源缺失或connector都仍占用原输入，connector={connector}"
        );
        assert_eq!(count, 0, "未核验来源不能包装成可答问题");
        assert_eq!(
            switched_state,
            Some(runtime::AgentTaskState::WaitingQuestion),
            "A/B均待答时，切会话后UI不能错误显示Ready"
        );
        assert!(saved_sessions.iter().any(|(id,_,session)| *id == node && session == "next-thread"), "真实原问题证明新会话已有历史，无代答权限也必须保存该session，而不是旧会话：{saved_sessions:?}");
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.has_pending_question(node) && Instant::now() < deadline {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !runtime.has_pending_question(node),
            "原工具消费后解除阻塞，重放不重新锁住输入"
        );
        assert!(
            runtime.questions().is_empty(),
            "消费也不能制造虚假的代答记录"
        );
    }
}
