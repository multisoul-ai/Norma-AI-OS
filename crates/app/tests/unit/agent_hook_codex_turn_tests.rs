use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use norma::codex::{native_questions::CodexQuestionHost, relay::CodexQuestionRelay};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
#[path = "../../../norma/tests/support/codex_relay.rs"]
mod transport;
use transport::{read, send, timed};

/// 场景：App 的生产 drain 在后台终端等待时不得结算任务或通知幕僚完成。
/// 构造：真实 Unix relay + 测试 PTY；Hook 提前 Stop，原生终态稍后才到。
/// 执行：提交 → 提前 Stop → 跨 30 分钟空轮询 → 工具结束 → 最终回复与原生终态。
/// 预期：等待期无 terminal_tasks/Done；真实结束只结算一次并携带最终回复。
#[test]
fn app_drain_waits_for_native_codex_completion_and_publishes_it_once() {
    let dir = tempfile::Builder::new()
        .prefix("nct-app-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let worker = std::thread::spawn(move || CodexQuestionRelay::connect(bridge, &path).unwrap());
    let mut provider = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
    let mut native = tungstenite::client("ws://localhost/", timed(native))
        .unwrap()
        .0;
    let relay = worker.join().unwrap();
    let mut runtime = Runtime::new_for_workspace(dir.path());
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "r".into(),
        norma_socket: "/tmp/n.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
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
        .adopt_codex_question_host(CodexQuestionHost::start(relay, node, process).unwrap())
        .unwrap();
    let (sender, receiver) = unbounded();
    let server = hook_server_for_test(receiver);
    send(
        &mut native,
        json!({"id":"start","method":"thread/start","params":{}}),
    );
    assert_eq!(
        read(&mut provider)["method"],
        "thread/start",
        "保留原 TUI 建会话"
    );
    send(
        &mut provider,
        json!({"id":"start","result":{"thread":{"id":"thread-a"}}}),
    );
    read(&mut native);
    let deadline = Instant::now() + Duration::from_secs(2);
    while runtime.agent_session_id(node).as_deref() != Some("thread-a") {
        server.drain_into_runtime(&mut runtime, Some("p"));
        assert!(Instant::now() < deadline, "App 必须接入原会话");
        std::thread::sleep(Duration::from_millis(5));
    }
    send(
        &mut provider,
        json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-a"}}}),
    );
    read(&mut native);
    let hook = |name, text| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({
                "hook_event_name": name, "session_id":"thread-a", "turn_id":"turn-a",
                "prompt":text, "last_assistant_message":text,
            }),
        )
        .unwrap()
    };
    sender
        .send(hook("UserPromptSubmit", "build and wait for its terminal"))
        .unwrap();
    let started = server.drain_into_runtime_at(&mut runtime, Some("p"), 1_000);
    assert_eq!(started.started_tasks.len(), 1, "真实提交仍须登记任务");
    sender
        .send(hook("Stop", "Waiting for background terminal"))
        .unwrap();
    for now in [2_000, 30 * 60 * 1000, 60 * 60 * 1000] {
        let held = server.drain_into_runtime_at(&mut runtime, Some("p"), now);
        assert!(
            held.terminal_tasks.is_empty(),
            "静默等待不能结算任务，now={now}"
        );
        assert!(
            held.events.iter().all(|e| e.provider_event != "Stop"),
            "未确认的 Stop 不得进入播报和幕僚结果路径"
        );
        assert_eq!(
            runtime.agent_task_state(node),
            Some(AgentTaskState::Thinking),
            "API/后台终端等待期间必须保持活跃"
        );
    }
    for event in [
        json!({"method":"item/completed","params":{"threadId":"thread-a","turnId":"turn-a","item":{"type":"commandExecution","status":"completed"}}}),
        json!({"method":"item/completed","params":{"threadId":"thread-a","turnId":"turn-a","item":{"type":"agentMessage","phase":"final_answer","text":"build verified"}}}),
        json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
    ] {
        send(&mut provider, event);
        read(&mut native);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let done = loop {
        let report = server.drain_into_runtime(&mut runtime, Some("p"));
        if !report.terminal_tasks.is_empty() {
            break report;
        }
        assert!(Instant::now() < deadline, "真实原生终态必须结算");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(done.terminal_tasks.len(), 1, "真实完成仅结算一次");
    assert_eq!(
        done.terminal_tasks[0].settled_state,
        AgentTaskState::Done,
        "只在真实完成后发布 Done"
    );
    assert_eq!(
        done.terminal_tasks[0].result.text.as_deref(),
        Some("build verified"),
        "幕僚必须得到最终回复，不能复用提前的等待文案"
    );
    assert!(
        server
            .drain_into_runtime(&mut runtime, Some("p"))
            .terminal_tasks
            .is_empty(),
        "空轮询不得重复触发完成副作用"
    );
    runtime.kill_all();
    let _ = native.close(None);
    let _ = provider.close(None);
}
