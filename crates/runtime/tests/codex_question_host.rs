#![cfg(unix)]
use norma::codex::native_questions::CodexQuestionHost;
use norma::codex::relay::CodexQuestionRelay;
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, Runtime};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "support/codex_relay.rs"]
mod support;
use support::{read, send, timed};

/// 场景：宿主桥在Runtime登记原连接与题目，采用或接管都只操作当前原问题。
/// 构造：录制PTY身份与真实Unix relay，原TUI自行thread/start并产生M0问答。
/// 执行：adopt受控host，轮询至可答，提交Green或回原卡片处理，再读真实RPC/消费。
/// 预期：未由JSON重绑连接，发送与消费均保留原delivery，取消无RPC，原TUI继续。
#[test]
fn runtime_adopts_owned_host_and_completes_original_question() {
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
                serde_json::from_str(include_str!(
                    "../../norma/tests/fixtures/questions/codex-source.json"
                ))
                .unwrap(),
            );
            send(
                &mut socket,
                serde_json::from_str(include_str!(
                    "../../norma/tests/fixtures/questions/codex-request.json"
                ))
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
                    serde_json::from_str(include_str!(
                        "../../norma/tests/fixtures/questions/codex-consumed.json"
                    ))
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
        let mut runtime = Runtime::new_for_workspace("/tmp/codex-host-test");
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "route".into(),
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
        let host = CodexQuestionHost::start(relay, node, process).unwrap();
        runtime.adopt_codex_question_host(host).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.questions().is_empty() && Instant::now() < deadline {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        let target = runtime.questions()[0].question.target.clone();
        assert_eq!(
            target.session_id, "codex-question-test-thread",
            "独立附着事件建立原生会话"
        );
        assert!(
            runtime.native_question_available(&target),
            "只在原连接可用时开放采用"
        );
        if cancel {
            runtime.release_question_to_native(&target).unwrap();
            assert!(
                !runtime.native_question_available(&target),
                "接管后旧建议不可发送"
            );
        } else {
            let answers = vec![QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-1".into()],
                text: None,
            }];
            let submission = runtime
                .claim_question_answer(&target, answers, "delivery")
                .unwrap()
                .unwrap();
            runtime.deliver_native_question(&submission).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while runtime.question_snapshot(&target).unwrap().state != QuestionState::Answered
                && Instant::now() < deadline
            {
                runtime.drain_native_question_hosts("p");
                std::thread::sleep(Duration::from_millis(5));
            }
            let snapshot = runtime.question_snapshot(&target).unwrap();
            assert_eq!(
                snapshot.state,
                QuestionState::Answered,
                "原RPC消费确认同一次提交"
            );
            assert_eq!(
                snapshot.delivery_id.as_deref(),
                Some("delivery"),
                "不替换持久化提交身份"
            );
            assert!(!runtime.has_pending_question(node), "消费后才解除阻塞");
        }
        continue_tx.send(!cancel).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
    }
}

/// 场景：仅问答观察器失效时，原TUI仍待答；不能因共享ticket提前取消而丢掉输入阻塞。
/// 构造：原题发出后重放矛盾来源使观察器停用；分别在Runtime首次drain前/后触发。
/// 执行：保持原WebSocket运行，检查不可采用但阻止新任务，最后由原TUI回答并继续通讯。
/// 预期：先保留Unknown，精确人工消费后转AnsweredElsewhere并解除阻塞，不能永久卡住。
#[test]
fn observer_failure_keeps_original_question_blocking_before_and_after_registration() {
    for (before_drain, new_session) in [(true, false), (false, false), (true, true), (false, true)]
    {
        let dir = tempfile::Builder::new()
            .prefix("nqh-")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (native, bridge) = UnixStream::pair().unwrap();
        let (stop, stopped) = crossbeam_channel::bounded(1);
        let (finish, finished) = crossbeam_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
            read(&mut socket);
            send(
                &mut socket,
                json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
            );
            let mut source: serde_json::Value = serde_json::from_str(include_str!(
                "../../norma/tests/fixtures/questions/codex-source.json"
            ))
            .unwrap();
            send(&mut socket, source.clone());
            send(
                &mut socket,
                serde_json::from_str(include_str!(
                    "../../norma/tests/fixtures/questions/codex-request.json"
                ))
                .unwrap(),
            );
            stopped.recv_timeout(Duration::from_secs(5)).unwrap();
            source["params"]["item"]["name"] = "mcp__connector__approval".into();
            send(&mut socket, source);
            if new_session {
                assert_eq!(
                    read(&mut socket)["method"],
                    "thread/start",
                    "原TUI自行切换会话"
                );
                send(
                    &mut socket,
                    json!({"id":"next","result":{"thread":{"id":"next-thread"}}}),
                );
            } else {
                assert_eq!(
                    read(&mut socket),
                    json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
                    "只透传原TUI的人工答案"
                );
                send(
                    &mut socket,
                    serde_json::from_str(include_str!(
                        "../../norma/tests/fixtures/questions/codex-consumed.json"
                    ))
                    .unwrap(),
                );
            }
            assert_eq!(
                read(&mut socket)["method"],
                "continued",
                "观察器故障不能自动回答原问题"
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
            for _ in 0..4 {
                read(&mut socket);
            }
            finished.recv_timeout(Duration::from_secs(5)).unwrap();
            if new_session {
                send(
                    &mut socket,
                    json!({"id":"next","method":"thread/start","params":{}}),
                );
                assert_eq!(
                    read(&mut socket)["result"]["thread"]["id"],
                    "next-thread",
                    "原TUI已切换到B"
                );
            } else {
                send(
                    &mut socket,
                    json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
                );
                assert_eq!(
                    read(&mut socket)["params"]["item"]["type"],
                    "function_call_output",
                    "原TUI收到人工答案消费"
                );
            }
            send(&mut socket, json!({"id":"continue","method":"continued"}));
            assert_eq!(
                read(&mut socket)["id"],
                "continue",
                "故障后原连接仍可人工操作"
            );
            let _ = socket.close(None);
        });
        let mut runtime = Runtime::new_for_workspace("/tmp/codex-host-test");
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "route".into(),
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
        let host = CodexQuestionHost::start(relay, node, process).unwrap();
        if before_drain {
            stop.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while host.is_available() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!host.is_available(), "首次drain前已确实停用观察器");
        }
        runtime.adopt_codex_question_host(host).unwrap();
        if !before_drain {
            let deadline = Instant::now() + Duration::from_secs(3);
            while runtime.questions().is_empty() && Instant::now() < deadline {
                runtime.drain_native_question_hosts("p");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!runtime.questions().is_empty(), "先登记原题再使观察器失效");
            stop.send(()).unwrap();
        }
        for _ in 0..20 {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        let blocks = runtime.has_pending_question(node);
        let questions = runtime.questions();
        let available = questions
            .first()
            .is_some_and(|q| runtime.native_question_available(&q.question.target));
        finish.send(()).unwrap();
        tui.join().unwrap();
        server.join().unwrap();
        assert!(
            blocks,
            "观察器失效不等于原问题结束，before_drain={before_drain}"
        );
        assert_eq!(questions.len(), 1, "保留原问题供用户接管");
        assert_eq!(
            questions[0].state,
            QuestionState::Unknown,
            "只撤销代答，原问题仍待确认"
        );
        assert!(!available, "未知来源不能代答");
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.has_pending_question(node) && Instant::now() < deadline {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        if new_session {
            assert_eq!(
                runtime.agent_session_id(node).as_deref(),
                Some("next-thread"),
                "停用代答后仍跟随原TUI可信附着"
            );
        } else {
            assert_eq!(
                runtime
                    .question_snapshot(&questions[0].question.target)
                    .unwrap()
                    .state,
                QuestionState::AnsweredElsewhere,
                "停用代答后仍需消费对账，before_drain={before_drain}"
            );
        }
        assert!(
            !runtime.has_pending_question(node),
            "人工回答完成不能永久阻塞后续任务"
        );
    }
}
