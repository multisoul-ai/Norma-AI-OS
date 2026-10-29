use norma::opencode::{client::OpenCodeQuestionClient, native_questions::OpenCodeQuestionHost};
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, Runtime};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// 场景：默认OpenCode卡片的run与受控instance宿主绑定，采用/人工接管不得变成普通任务。
/// 构造：可写echo PTY与仅原question/message路由的本机HTTP服务，真实三题夹具。
/// 执行：原run采用host→原题占有输入→提交或先撤销→真实HTTP结果对账。
/// 回包丢失分支：同工具消费只关闭原题，不保留我方交付归属，也不得重发。
/// 预期：原会话原题Answered且唯一POST，接管零POST；错误process不能接管，普通任务始终被原题挡住。
#[test]
fn owned_opencode_host_routes_current_question_through_runtime() {
    for (cancel, drop_reply) in [(false, false), (true, false), (false, true)] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let posts = Arc::new(AtomicUsize::new(0));
        let (worker_stop, worker_posts) = (stop.clone(), posts.clone());
        let server = std::thread::spawn(move || {
            'connections: while !worker_stop.load(Ordering::Acquire) {
                let mut socket = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("本机服务失败:{e}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() || first.is_empty() {
                    continue;
                }
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        continue 'connections;
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(length < 8192, "答案长度有界");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let value = if first.starts_with("GET /question?") {
                    if worker_posts.load(Ordering::Acquire) > 0 {
                        json!([])
                    } else {
                        json!([serde_json::from_str::<Value>(include_str!(
                            "../../norma/tests/fixtures/questions/opencode-request.json"
                        ))
                        .unwrap()])
                    }
                } else if first.starts_with("POST /question/") {
                    let body: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        body["answers"],
                        json!([["Green"], ["macOS", "Linux"], ["NATIVE_OPENCODE_7F2"]]),
                        "HTTP仅回答原题"
                    );
                    worker_posts.fetch_add(1, Ordering::AcqRel);
                    if drop_reply {
                        continue;
                    }
                    json!(true)
                } else if first.starts_with("GET /session/") {
                    json!({"parts":[serde_json::from_str::<Value>(include_str!("../../norma/tests/fixtures/questions/opencode-consumed.json")).unwrap()]})
                } else {
                    panic!("不允许新任务路径:{first}")
                };
                let body = value.to_string();
                let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len());
            }
        });
        let mut runtime = Runtime::new_for_workspace("/tmp/opencode-owned-test");
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:9/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "r".into(),
            norma_socket: "/tmp/unused.sock".into(),
        }));
        runtime.set_pty_factory(Box::new(|_, _, _, sender| {
            Ok(Box::new(terminal::TestPtyBackend::echo(sender)))
        }));
        let node = runtime
            .spawn_agent(
                canvas::AgentProfile::OpenCode,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .unwrap();
        let process = runtime
            .mobile_agent_run_id(node)
            .expect("原生OpenCode必须保留进程run")
            .to_owned();
        let client = || {
            OpenCodeQuestionClient::new(
                &endpoint,
                "/tmp/opencode-owned-test",
                "ses_e818a6dfbffehqPKTZcQvWJCLL",
                "opencode",
                "test-secret",
            )
            .unwrap()
        };
        let wrong = OpenCodeQuestionHost::start(client(), node, "wrong-process".into()).unwrap();
        assert!(
            runtime.adopt_opencode_question_host(wrong, "p").is_err(),
            "别的进程不能覆盖当前实例"
        );
        let host = OpenCodeQuestionHost::start(client(), node, process).unwrap();
        runtime.adopt_opencode_question_host(host, "p").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.questions().is_empty() && Instant::now() < deadline {
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(5));
        }
        let target = runtime.questions()[0].question.target.clone();
        assert_eq!(
            target.session_id, "ses_e818a6dfbffehqPKTZcQvWJCLL",
            "绑定原instance指定session"
        );
        let child_session = norma::agent_hook::AgentHookEvent {
            node_id: node,
            profile: norma::protocol::AgentProfile::OpenCode,
            control_run_id: None,
            provider_event: "SessionStart".into(),
            session_id: Some("ses_child_agent".into()),
            turn_id: None,
            tool_name: None,
            message: None,
            raw_json: json!({"source":"startup","parentID":target.session_id}),
        };
        assert!(
            !runtime.observe_mobile_hook(&child_session),
            "子Agent创建session不能把原TUI切到子会话"
        );
        assert_eq!(
            runtime.agent_session_id(node).as_deref(),
            Some(target.session_id.as_str()),
            "受控宿主的原会话归属不能被普通Hook覆盖"
        );
        assert!(runtime.native_question_available(&target), "原问题可以提交");
        assert!(
            runtime
                .send_agent_task_text(node, "must not become task")
                .is_err(),
            "原题未解不得派发任务"
        );
        if cancel {
            runtime.release_question_to_native(&target).unwrap();
            assert!(
                !runtime.native_question_available(&target),
                "接管撤销旧答案资格"
            );
            assert!(runtime.has_pending_question(node), "接管并不假称问题结束");
        } else {
            let answers = vec![
                QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                },
                QuestionAnswer {
                    key: "question-1".into(),
                    selected: vec!["option-0".into(), "option-2".into()],
                    text: None,
                },
                QuestionAnswer {
                    key: "question-2".into(),
                    selected: vec![],
                    text: Some("NATIVE_OPENCODE_7F2".into()),
                },
            ];
            let submission = runtime
                .claim_question_answer(&target, answers, "delivery")
                .unwrap()
                .unwrap();
            runtime.deliver_native_question(&submission).unwrap();
            let expected = if drop_reply {
                QuestionState::ConsumedUnattributed
            } else {
                QuestionState::Answered
            };
            let deadline = Instant::now() + Duration::from_secs(5);
            while runtime.question_snapshot(&target).unwrap().state != expected
                && Instant::now() < deadline
            {
                runtime.drain_native_question_hosts("p");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                runtime.question_snapshot(&target).unwrap().state,
                expected,
                "必须等待原工具精确消费"
            );
            assert!(!runtime.has_pending_question(node), "消费后输入锁解除");
            assert!(
                !runtime.observe_mobile_hook(&child_session),
                "空闲时子Agent创建session也不能替换原TUI归属"
            );
            assert_eq!(
                runtime.agent_session_id(node).as_deref(),
                Some(target.session_id.as_str()),
                "已答后仍绑定原TUI session"
            );
            assert_eq!(
                runtime
                    .question_snapshot(&target)
                    .unwrap()
                    .delivery_id
                    .as_deref(),
                if drop_reply { None } else { Some("delivery") },
                "丢失 HTTP 接纳不能冒认本次交付"
            );
            assert!(
                runtime.deliver_native_question(&submission).is_err(),
                "完成后不能再次提交"
            );
        }
        runtime.kill_all();
        drop(runtime);
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        assert_eq!(
            posts.load(Ordering::Acquire),
            usize::from(!cancel),
            "采用一次POST，接管零POST"
        );
    }
}
