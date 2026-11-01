#![cfg(unix)]

use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{parse_hook_event, HookSource};
use norma::codex::{native_questions::CodexQuestionHost, relay::CodexQuestionRelay};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime, RuntimeEvent};
use serde_json::json;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};
use tungstenite::WebSocket;

#[path = "../../norma/tests/support/codex_relay.rs"]
mod transport;
use transport::{read, send, timed};

fn connection() -> (
    WebSocket<UnixStream>,
    WebSocket<UnixStream>,
    CodexQuestionRelay,
) {
    let dir = tempfile::Builder::new()
        .prefix("nct-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (native, bridge) = UnixStream::pair().unwrap();
    let worker = std::thread::spawn(move || CodexQuestionRelay::connect(bridge, &path).unwrap());
    let server = tungstenite::accept(timed(listener.accept().unwrap().0)).unwrap();
    let native = tungstenite::client("ws://localhost/", timed(native))
        .unwrap()
        .0;
    (native, server, worker.join().unwrap())
}

fn drain_until(runtime: &mut Runtime, node: u64, expected: AgentTaskState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        runtime.drain_native_question_hosts("p");
        if runtime.agent_task_state(node) == Some(expected.clone()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "原生回合应进入 {expected:?}，实际 {:?}",
            runtime.agent_task_state(node)
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 场景：Codex 等待后台终端与 API 响应期间仍属同一活跃回合，Stop Hook 不是终态证据。
/// 构造：真实 Unix relay，thread-a/turn-a；原生 turn/started 后注入提前 Stop；
///       commandExecution 完成、后台终端交互及另一线程完成均不能终结 turn-a。
/// 执行：接入原生宿主 → 开始回合 → 模拟等待及提前 Hook → 原生 turn/completed。
/// 预期：等待期间始终活跃且无 Done 事件；只有匹配的原生终态才可完成。
#[test]
fn background_terminal_wait_stays_active_until_native_turn_completes() {
    terminal_wait("completed", AgentTaskState::Done);
}

/// 场景：修复提前完成不能吞掉真实失败或中断。
/// 构造：复用同一后台终端等待链路，分别用 failed/interrupted 终结原生回合。
/// 执行：等待期拒绝 Stop，随后接收匹配终态。
/// 预期：失败保留结构化错误，中断为 Ended；两者均不产生 Done。
#[test]
fn native_failure_and_interrupt_do_not_become_success() {
    terminal_wait(
        "failed",
        AgentTaskState::Failed {
            message: "Codex turn failed".into(),
        },
    );
    terminal_wait("interrupted", AgentTaskState::Ended);
}

fn terminal_wait(status: &str, expected: AgentTaskState) {
    let mut runtime = Runtime::new_for_workspace("/tmp/codex-turn-test");
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
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let (mut native, mut server, relay) = connection();
    runtime
        .adopt_codex_question_host(CodexQuestionHost::start(relay, node, process).unwrap())
        .unwrap();
    send(
        &mut native,
        json!({"id":"start","method":"thread/start","params":{}}),
    );
    assert_eq!(
        read(&mut server)["method"],
        "thread/start",
        "由原 TUI 建立会话"
    );
    send(
        &mut server,
        json!({"id":"start","result":{"thread":{"id":"thread-a"}}}),
    );
    read(&mut native);
    send(
        &mut server,
        json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-a"}}}),
    );
    read(&mut native);
    runtime.drain_native_question_hosts("p");
    let prompt = parse_hook_event(HookSource::Codex, node, json!({
        "hook_event_name":"UserPromptSubmit", "session_id":"thread-a", "turn_id":"turn-a", "prompt":"run build and wait"
    })).unwrap();
    assert!(runtime.observe_mobile_hook(&prompt), "提交当前回合应被接受");
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: prompt.provider_event,
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: prompt.session_id,
    });
    drain_until(&mut runtime, node, AgentTaskState::Thinking);
    runtime.take_events();

    let stop = parse_hook_event(
        HookSource::Codex,
        node,
        json!({
            "hook_event_name":"Stop", "session_id":"thread-a", "turn_id":"turn-a"
        }),
    )
    .unwrap();
    assert!(
        !runtime.observe_mobile_hook(&stop),
        "原生回合仍在执行时不得接受提前的 Stop Hook"
    );
    for event in [
        json!({"method":"item/completed","params":{"threadId":"thread-a","turnId":"turn-a","item":{"type":"agentMessage","phase":"commentary","text":"Waiting for background terminal"}}}),
        json!({"method":"item/completed","params":{"threadId":"thread-a","turnId":"turn-a","item":{"type":"commandExecution","status":"completed"}}}),
        json!({"method":"item/commandExecution/terminalInteraction","params":{"threadId":"thread-a","turnId":"turn-a","processId":"123","stdin":""}}),
        json!({"method":"turn/completed","params":{"threadId":"other-thread","turn":{"id":"turn-a","status":"completed"}}}),
    ] {
        send(&mut server, event.clone());
        assert_eq!(read(&mut native), event, "后台等待和子会话事件必须原样透传");
    }
    runtime.drain_native_question_hosts("p");
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "等待 API/后台终端时保留活跃状态"
    );
    assert!(
        !runtime.take_events().iter().any(|e| matches!(
            e,
            RuntimeEvent::AgentTaskStateChanged {
                to: AgentTaskState::Done,
                ..
            }
        )),
        "幕僚不得收到伪完成事件"
    );

    send(
        &mut server,
        json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":status}}}),
    );
    read(&mut native);
    drain_until(&mut runtime, node, expected.clone());
    let events = runtime.take_events();
    assert!(
        events.iter().any(
            |e| matches!(e, RuntimeEvent::AgentTaskStateChanged { to, .. } if to == &expected)
        ),
        "真实终态仍须通知幕僚"
    );
    if status != "completed" {
        assert!(
            !events.iter().any(|e| matches!(
                e,
                RuntimeEvent::AgentTaskStateChanged {
                    to: AgentTaskState::Done,
                    ..
                }
            )),
            "失败和中断不能向幕僚报成功"
        );
    }
    let updates = runtime.take_native_agent_task_updates();
    assert_eq!(updates.len(), 1, "每个真实终态只发布一份 App 副作用");
    assert!(
        runtime
            .native_mobile_event(&updates[0].0)
            .and_then(|(_, _, outcome)| outcome)
            .is_some(),
        "真实终态仍须保留移动会话持久化证据"
    );
    runtime.kill_all();
    let _ = native.close(None);
    let _ = server.close(None);
}
