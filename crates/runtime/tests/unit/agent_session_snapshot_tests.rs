use crate::{AgentTaskState, AgentTaskUpdate, Runtime};
use canvas::{AgentProfile, CanvasRect, NodeId};
use terminal::PtyCommand;

fn bounds() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 640.0, 400.0)
}

fn update(node_id: NodeId, to: AgentTaskState, provider_event: &str) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: provider_event.to_string(),
        to,
        task_title: None,
        session_id: None,
    }
}

/// Agent task update 应缓存 hook 提供的 session_id，关闭窗口保存 node_sessions 时不得再依赖扫磁盘发现。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1，由 spawn_agent_with_command 创建真实 live node
///   update.session_id="codex-session-from-hook"，来自 hook JSON 的 session_id 字段
///   runtime workspace_root=None，因此无法通过 ~/.codex discovery 兜底，保存结果只能来自缓存
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Codex Agent live node
///   2. apply_agent_task_update 写入 Thinking + session_id
///   3. save_node_sessions 写入临时 sqlite
///   4. load_sessions 读取保存的 node session
///
/// 预期结果：
///   - 正断言：runtime.agent_session_id(node_id)=Some("codex-session-from-hook")
///   - 正断言：node_sessions 中该 Agent 的 session_id 被保存
///   - 负断言：session_id 不应为 None，否则关闭路径还会被迫扫 ~/.codex/sessions
#[test]
fn test_agent_task_update_caches_session_id_for_node_session_snapshot() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("test should create workspace dir");
    std::fs::create_dir(workspace.join(".multisoul"))
        .expect("test should create project store dir");
    let project_store =
        storage::ProjectStore::for_workspace(&workspace).expect("project store should open");
    project_store
        .migrate()
        .expect("project store migration should create schema");
    let store = project_store.node_session_store();
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "sleep 0.1"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Codex, bounds(), command, 80, 24)
        .expect("测试 Agent 替身应可成功 spawn");
    let _registered = runtime.take_events();
    let mut task_update = update(node_id, AgentTaskState::Thinking, "UserPromptSubmit");
    task_update.session_id = Some("codex-session-from-hook".to_string());

    let changed = runtime.apply_agent_task_update(task_update);
    runtime
        .save_node_sessions(&store)
        .expect("node sessions should save without session discovery");
    let sessions = store
        .load_sessions()
        .expect("saved node sessions should load");
    let saved = sessions
        .iter()
        .find(|session| session.node_id == node_id)
        .expect("Agent node session should be persisted");

    assert!(changed, "带 session_id 的 task update 仍应更新 task state");
    assert_eq!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("codex-session-from-hook"),
        "runtime 应缓存 hook session_id，供关闭时快速保存"
    );
    assert_eq!(
        saved.session_id.as_deref(),
        Some("codex-session-from-hook"),
        "node_sessions 应保存 hook session_id，而不是留空后依赖磁盘 discovery"
    );
    assert!(
        saved.session_id.is_some(),
        "session_id 不应为 None，否则关闭窗口会再次触发昂贵 session discovery"
    );

    runtime.kill_all();
}
