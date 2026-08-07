use super::*;
use canvas::{AgentSpec, CanvasRect};

fn session_start(node_id: u64, session_id: &str) -> AgentHookEvent {
    norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": session_id
        }),
    )
    .expect("测试 SessionStart 应可解析")
}

/// Hook drain 必须把纯 session 绑定变化单独报告给持久化调度器。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent = 1 张，node_id=1；
///   drain 次数 = 2；两次 task state 均为 Ready；
///   session_id 依次为 `session-a`、`session-b`，绑定变化次数 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向测试 Hook channel 发送 session-a，第一次 drain 写入初始绑定；
///   2. 保持 Ready，再发送 session-b 并第二次 drain；
///   3. 检查 drain report 与 Runtime 最终绑定。
///
/// 预期结果：
///   - 正断言：两次 report 都标记 session_binding_changed；
///   - 正断言：第二次 report.changed=true，可触发正常 frame 更新；
///   - 正断言：Runtime 最终绑定 session-b；
///   - 负断言：最终绑定不得停留在 session-a。
#[test]
fn drain_reports_session_binding_change_when_task_state_stays_ready() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: CanvasAgentProfile::Codex,
            alias: None,
            definition_id: None,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    let (sender, receiver) = unbounded();
    #[cfg(feature = "e2e-test-support")]
    let event_sender = sender.clone();
    let server = AgentHookServer {
        receiver,
        #[cfg(feature = "e2e-test-support")]
        event_sender,
        endpoint: None,
        token: None,
    };

    sender
        .send(session_start(node_id, "session-a"))
        .expect("第一次 SessionStart 应进入测试 channel");
    let first = server.drain_into_runtime(&mut runtime);
    sender
        .send(session_start(node_id, "session-b"))
        .expect("第二次 SessionStart 应进入测试 channel");
    let second = server.drain_into_runtime(&mut runtime);

    assert!(
        first.session_binding_changed,
        "第一次 SessionStart 应报告初始 session 绑定变化"
    );
    assert!(
        second.session_binding_changed,
        "Ready 状态未变时，session-a→session-b 仍应单独报告绑定变化"
    );
    assert!(
        second.changed,
        "纯 session 变化也属于 runtime change，frame loop 不得忽略"
    );
    assert_eq!(
        first.ready_node_ids,
        vec![node_id],
        "首次 None→Ready 应恰好报告一次 ready，供等待任务继续执行"
    );
    assert!(
        second.ready_node_ids.is_empty(),
        "第二次 Ready 状态未变，纯 session 变化不得重复推进 Ready 队列"
    );
    assert_eq!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-b"),
        "第二次 drain 后 Runtime 应绑定 session-b"
    );
    assert_ne!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-a"),
        "第二次 drain 后不得继续保留 session-a"
    );
}
