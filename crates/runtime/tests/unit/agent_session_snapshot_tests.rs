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
    let project_store =
        storage::ProjectStore::for_database_paths(
            &storage::DatabasePaths::isolated_test(workspace.parent().expect("测试项目应有父目录")),
            &workspace,
        ).expect("project store should open");
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

/// DWS-05：两张 Agent 卡片可同时选择同一 worktree，启动目录和会话存档各自保留。
///
/// 数据构造（含关键数值的推导过程）：
///   项目根目录 = 1；共享执行目录 = 1；Agent 数 = 0 + 2 = 2。
///   预期 PTY 启动记录 = 2，node_sessions 中带共享 cwd 的 Agent = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Runtime 注入测试 PTY 工厂并创建两个选同一 worktree 的 Codex Agent。
///   2. 保存 node_sessions 并重新读取。
///   3. 检查两个节点 ID、PTY cwd 和持久化 cwd。
///
/// 预期结果：
///   - 正断言：两个 Agent 均启动并保存相同执行目录。
///   - 负断言：第二次创建不得复用首个节点或被 worktree 锁拒绝。
#[test]
fn desktop_agents_can_share_one_worktree_with_independent_sessions() {
    use std::sync::{Arc, Mutex};
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let root = temp.path().join("project");
    let worktree = temp.path().join("linked");
    std::fs::create_dir(&root).expect("应创建项目目录");
    std::fs::create_dir(&worktree).expect("应创建执行目录");
    let store = storage::ProjectStore::initialize_for_database_paths(
        &storage::DatabasePaths::isolated_test(temp.path()), &root,
    ).expect("应初始化项目数据库").node_session_store();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_factory = observed.clone();
    let mut runtime = Runtime::new_for_workspace(root);
    runtime.set_pty_factory(Box::new(move |command, _cols, _rows, tx| {
        observed_for_factory.lock().expect("PTY 记录锁不应损坏").push(command.cwd.clone());
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    }));
    let first = runtime.spawn_agent_with_worktree(AgentProfile::Codex, Some("First".into()), bounds(), 80, 24, &worktree).expect("首个 Agent 应启动");
    let second = runtime.spawn_agent_with_worktree(AgentProfile::Codex, Some("Second".into()), bounds(), 80, 24, &worktree).expect("第二个 Agent 应共享 worktree 启动");
    runtime.save_node_sessions(&store).expect("应保存两张 Agent 的目录");
    let sessions = store.load_sessions().expect("应重读 Agent 会话存档");
    let observed = observed.lock().expect("PTY 记录锁不应损坏");

    assert_ne!(first, second, "共享 worktree 的两张 Agent 卡片必须有不同节点 ID");
    assert_eq!(observed.len(), 2, "两张 Agent 卡片都应各自启动 PTY");
    assert_eq!(observed[0].as_deref(), Some(worktree.as_path()), "首个 Agent 必须在所选 worktree 启动");
    assert_eq!(observed[1].as_deref(), Some(worktree.as_path()), "第二个 Agent 必须也在同一 worktree 启动");
    assert_eq!(sessions.iter().find(|item| item.node_id == first).and_then(|item| item.cwd.as_deref()), worktree.to_str(), "首个 Agent cwd 应持久化");
    assert_eq!(sessions.iter().find(|item| item.node_id == second).and_then(|item| item.cwd.as_deref()), worktree.to_str(), "第二个 Agent cwd 应独立持久化");
    assert!(!sessions.iter().any(|item| item.node_id != first && item.node_id != second), "存档不得出现额外 Agent 会话");
}
