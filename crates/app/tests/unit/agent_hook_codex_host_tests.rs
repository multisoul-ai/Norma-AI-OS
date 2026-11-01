use super::*;
use norma::codex::{native_questions::CodexQuestionHost, relay::CodexQuestionRelay};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "../../../norma/tests/support/codex_relay.rs"]
mod relay_support;
use relay_support::{read, send, timed};

/// 场景：生产App的统一drain也要处理同进程宿主，即使HTTP Hook队列为空。
/// 构造：空Hook server，测试PTY身份，真实Unix relay透传原start与问答事件。
/// 执行：仅调用App drain_into_runtime，等题目登记并检查session持久化通知。
/// 预期：原题可见且绑定到p；报告连接变化；第二次空drain不反复脏写。
#[test]
fn app_hook_drain_adopts_native_questions_and_reports_session_changes() {
    let dir = tempfile::Builder::new()
        .prefix("nqa-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let (finish, finished) = crossbeam_channel::bounded(1);
    let provider = std::thread::spawn(move || {
        let mut socket = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
        assert_eq!(
            read(&mut socket)["method"],
            "thread/start",
            "原TUI自行创建会话"
        );
        send(
            &mut socket,
            json!({"id":"start","result":{"thread":{"id":"codex-question-test-thread"}}}),
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/codex-source.json"
            ))
            .unwrap(),
        );
        send(
            &mut socket,
            serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/codex-request.json"
            ))
            .unwrap(),
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
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = socket.close(None);
    });
    let mut runtime = Runtime::new_for_workspace("/tmp/app-question-test");
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
            CanvasAgentProfile::Codex,
            canvas::CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    runtime
        .adopt_codex_question_host(
            CodexQuestionHost::start(
                CodexQuestionRelay::connect(bridge, &path).unwrap(),
                node,
                process,
            )
            .unwrap(),
        )
        .unwrap();
    let (_, receiver) = unbounded();
    let server = hook_server_for_test(receiver);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut changed = false;
    let mut session_changed = false;
    while runtime.questions().is_empty() && Instant::now() < deadline {
        let report = server.drain_into_runtime(&mut runtime, Some("p"));
        changed |= report.changed;
        session_changed |= report.session_binding_changed;
        std::thread::sleep(Duration::from_millis(5));
    }
    let questions = runtime.questions();
    let clean = server.drain_into_runtime(&mut runtime, Some("p"));
    finish.send(()).unwrap();
    tui.join().unwrap();
    provider.join().unwrap();
    assert_eq!(
        questions.len(),
        1,
        "App必须轮询同进程问题宿主，不能只等HTTP Hook"
    );
    assert_eq!(
        questions[0].question.target.project_id, "p",
        "原问题属于当前项目"
    );
    assert!(changed, "原题出现触发UI刷新");
    assert!(session_changed, "可信原连接变化要通知session持久化");
    assert!(
        !clean.changed && !clean.session_binding_changed,
        "空轮询不能反复脏写"
    );
}
