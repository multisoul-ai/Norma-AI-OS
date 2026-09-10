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
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    let (sender, receiver) = unbounded();
    #[cfg(feature = "e2e-test-support")]
    let event_sender = sender.clone();
    let server = AgentHookServer {
        #[cfg(feature = "e2e-test-support")]
        event_tap: None,
        approvals: crossbeam_channel::bounded(1).1,
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

/// 连续PostToolUse即使保持Thinking，也必须报告进度变化以唤醒现有WSS合并通道。
///
/// 数据构造：1张Claude原卡，2个成功创建事件；两个事件后都是Thinking，计划总数从1变2。
/// 执行过程：提交 → 创建1 → 创建2 → 重放创建2。
/// 预期结果：第二次仅进度变化报告changed，重放不报告变化；不产生第二个回合或会话写入。
#[test]
fn drain_reports_progress_without_task_state_change() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            CanvasAgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原卡");
    let (sender, receiver) = unbounded();
    let server = AgentHookServer {
        #[cfg(feature = "e2e-test-support")]
        event_tap: None,
        #[cfg(feature = "e2e-test-support")]
        event_sender: sender.clone(),
        approvals: crossbeam_channel::bounded(1).1,
        receiver,
        endpoint: None,
        token: None,
    };
    let event = |name: &str, id: &str| {
        norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Claude, node, serde_json::json!({
            "hook_event_name":name, "session_id":"session-progress", "prompt_id":"turn-progress",
            "tool_name":"TaskCreate", "tool_use_id":id, "tool_input":{"subject":id},
            "tool_response":{"task":{"id":id,"subject":id}}
        })).expect("真实载荷形状")
    };
    sender
        .send(event("UserPromptSubmit", "start"))
        .expect("入队");
    server.drain_into_runtime(&mut runtime);
    sender.send(event("PostToolUse", "1")).expect("创建1");
    assert!(
        server.drain_into_runtime(&mut runtime).changed,
        "首个计划应唤醒"
    );
    sender.send(event("PostToolUse", "2")).expect("创建2");
    let next = server.drain_into_runtime(&mut runtime);
    assert!(next.changed, "Thinking不变仍必须立即进入250ms合并通道");
    assert!(
        !next.session_binding_changed,
        "显示变化不应触发持久化session"
    );
    assert!(next.started_tasks.is_empty(), "创建步骤不等于提交新回合");
    assert_eq!(
        runtime.mobile_task_progress(node).expect("计划").total,
        2,
        "两个步骤全部可见"
    );
    sender.send(event("PostToolUse", "2")).expect("重放");
    assert!(
        !server.drain_into_runtime(&mut runtime).changed,
        "重复事件不制造网络增量"
    );
}
