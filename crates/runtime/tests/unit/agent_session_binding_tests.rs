use crate::{AgentTaskState, AgentTaskUpdate, Runtime};
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, TermSpec};
use storage::{NodeSession, NodeSessionKind, SessionCheckpoint, SessionCheckpointKind};

fn add_agent(runtime: &mut Runtime, profile: AgentProfile) -> u64 {
    runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    )
}

fn agent_session(node_id: u64, session_id: &str) -> NodeSession {
    NodeSession {
        node_id,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some(session_id.to_string()),
    }
}

/// 同一 Provider 的重复历史 session 不得绑定给多张 Agent 卡片，独立绑定仍应恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent = 3 张，node_id 依次为 1、2、3；
///   node 1/2 的 session_id 都是 `shared-session`，冲突拥有者 = 2；
///   node 3 的 session_id 是 `unique-session`，唯一拥有者 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 Runtime document 添加三张 Codex Agent 卡片；
///   2. hydrate_agent_session_bindings 一次性加载三条持久化记录；
///   3. Runtime 按 `(profile, session_id)` 检查所有权，再暴露每个节点的绑定。
///
/// 预期结果：
///   - 正断言：node 3 保留 `unique-session`，证明合法历史仍可恢复；
///   - 负断言：node 1 和 node 2 都不得绑定 `shared-session`，避免任意选中一张卡；
///   - 正断言：报告精确记录 1 个有效绑定和 2 个冲突节点。
#[test]
fn duplicate_provider_session_is_quarantined_for_every_conflicting_agent_card() {
    let mut runtime = Runtime::new();
    let first = add_agent(&mut runtime, AgentProfile::Codex);
    let second = add_agent(&mut runtime, AgentProfile::Codex);
    let third = add_agent(&mut runtime, AgentProfile::Codex);
    let sessions = vec![
        agent_session(first, "shared-session"),
        agent_session(second, "shared-session"),
        agent_session(third, "unique-session"),
    ];

    let report = runtime.hydrate_agent_session_bindings(&sessions);

    assert_eq!(
        runtime.agent_session_id(third).as_deref(),
        Some("unique-session"),
        "唯一的 node 3 session 应被恢复，不能因其他冲突记录一起丢失"
    );
    assert_eq!(
        runtime.agent_session_id(first),
        None,
        "node 1 与 node 2 共享 session，node 1 不得继续绑定 shared-session"
    );
    assert_eq!(
        runtime.agent_session_id(second),
        None,
        "node 1 与 node 2 共享 session，node 2 不得继续绑定 shared-session"
    );
    assert_eq!(
        report.bound_count, 1,
        "三条记录中只有 unique-session 具备唯一所有权，应只恢复 1 条"
    );
    assert_eq!(
        report.conflicting_node_ids,
        vec![first, second],
        "冲突报告应逐一列出 node 1 和 node 2，不能遗漏或包含合法 node 3"
    );
    assert!(
        !report.conflicting_node_ids.contains(&third),
        "node 3 使用独立 session，不应被误报为冲突"
    );
}

/// Hook 只改变 session_id、任务状态保持不变时，也必须报告 Runtime 已变化。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent = 1 张，node_id=1；
///   update A/B 的 task state 都是 Ready，状态变化次数 = 1（首次 None→Ready）；
///   session 依次为 `session-a`、`session-b`，绑定变化次数 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次 apply_agent_task_update 写入 Ready + session-a；
///   2. 第二次保持 Ready，只把 Provider session 更新为 session-b；
///   3. 检查第二次返回值和 Runtime 最终绑定。
///
/// 预期结果：
///   - 正断言：两次 update 都返回 changed=true；
///   - 正断言：最终绑定为 session-b；
///   - 负断言：最终绑定不得停留在 session-a。
#[test]
fn session_only_hook_update_is_reported_as_runtime_change() {
    let mut runtime = Runtime::new();
    let node_id = add_agent(&mut runtime, AgentProfile::Codex);
    let update = |session_id: &str| AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: "SessionStart".to_string(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: Some(session_id.to_string()),
    };

    let first_changed = runtime.apply_agent_task_update(update("session-a"));
    let second_changed = runtime.apply_agent_task_update(update("session-b"));

    assert!(
        first_changed,
        "首次 SessionStart 应同时登记 Ready 和 session-a"
    );
    assert!(
        second_changed,
        "Ready 未变但 session-a→session-b 时仍应报告 changed，才能触发持久化"
    );
    assert_eq!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-b"),
        "第二个 SessionStart 应把同一卡片更新为 session-b"
    );
    assert_ne!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-a"),
        "Provider 已切换 session 后不得保留旧绑定 session-a"
    );
}

/// 延迟恢复尚未创建 live PTY 时，保存不得删除历史 Agent 绑定或 Terminal CWD。
///
/// 数据构造（含关键数值的推导过程）：
///   document = Terminal(node 1) + Codex Agent(node 2)，live node 数 = 0；
///   已保存 Terminal cwd=`/saved/worktree`；
///   已保存 Agent session=`agent-session-2`；另有已删除 node 99 的陈旧记录；
///   保存前 session 行数 = 3，当前 document 对应行数 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 ProjectStore 写入两条当前 node_sessions 和一条 node 99 陈旧记录；
///   2. Runtime 创建相同 document，并 hydrate Agent 绑定，但不启动任何 PTY；
///   3. 调用 Runtime::save_node_sessions，模拟延迟恢复期间的 dirty flush；
///   4. 通过 NodeSessionStore 公开接口重新加载结果。
///
/// 预期结果：
///   - 正断言：保存后仍有 2 条记录；
///   - 正断言：Terminal node 1 仍保留 `/saved/worktree`；
///   - 正断言：Agent node 2 仍保留 `agent-session-2`；
///   - 负断言：两条有效记录不得因 live node 数为 0 被删除，node 99 必须清理。
#[test]
fn save_during_deferred_restore_preserves_agent_binding_and_terminal_cwd() {
    let temp = tempfile::tempdir().expect("延迟恢复保存测试应创建临时目录");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("延迟恢复保存测试应创建 workspace");
    let project_store =
        storage::ProjectStore::for_database_paths(
            &storage::DatabasePaths::isolated_test(workspace.parent().expect("测试项目应有父目录")),
            &workspace,
        ).expect("ProjectStore 应可初始化");
    project_store
        .migrate()
        .expect("ProjectStore 应完成 node_sessions migration");
    let store = project_store.node_session_store();
    let mut runtime = Runtime::new();
    let terminal_id = runtime.document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    let agent_id = add_agent(&mut runtime, AgentProfile::Codex);
    let saved_terminal = NodeSession {
        node_id: terminal_id,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/saved/worktree".to_string()),
        session_id: None,
    };
    let saved_agent = agent_session(agent_id, "agent-session-2");
    store
        .save_session(&saved_terminal)
        .expect("Terminal CWD 应可预先写入");
    store
        .save_session(&saved_agent)
        .expect("Agent session 应可预先写入");
    store
        .save_session(&agent_session(99, "stale-session-99"))
        .expect("已删除 node 99 的陈旧记录应可预先写入");
    runtime.hydrate_agent_session_bindings(&[saved_terminal, saved_agent]);

    runtime
        .save_node_sessions(&store)
        .expect("延迟恢复期间 save_node_sessions 不应失败");
    let saved = store
        .load_sessions()
        .expect("保存后 node_sessions 应可通过公开接口读取");
    let terminal = saved
        .iter()
        .find(|session| session.node_id == terminal_id)
        .expect("保存后 Terminal node 1 应仍存在");
    let agent = saved
        .iter()
        .find(|session| session.node_id == agent_id)
        .expect("保存后 Agent node 2 应仍存在");

    assert_eq!(
        saved.len(),
        2,
        "document 仍有 Terminal 和 Agent，保存后应保留精确 2 条 node session"
    );
    assert_eq!(
        terminal.cwd.as_deref(),
        Some("/saved/worktree"),
        "Terminal 尚未 live 时应保留上次已知 CWD"
    );
    assert_eq!(
        agent.session_id.as_deref(),
        Some("agent-session-2"),
        "Agent 尚未 live 时应保留已 hydrate 的精确 session"
    );
    assert!(
        saved.iter().any(|session| session.node_id == terminal_id),
        "live node 数为 0 时不得删除 Terminal node session"
    );
    assert!(
        saved.iter().any(|session| session.node_id == agent_id),
        "live node 数为 0 时不得删除 Agent node session"
    );
    assert!(
        !saved.iter().any(|session| session.node_id == 99),
        "node 99 已不在 document 中，reconcile 必须删除陈旧绑定以防节点 ID 复用"
    );
}

/// Agent 恢复必须以 Canvas document 的 profile 为权威，不能让旧 checkpoint 改写 Provider。
///
/// 数据构造（含关键数值的推导过程）：
///   document profile = Codex；
///   checkpoint = Claude Agent(node 1)，模拟旧数据 profile 漂移；
///   Provider 候选数量 = 2，但 Canvas document 是当前节点类型的唯一权威来源。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造声明 Claude 的旧 Agent checkpoint；
///   2. 以 document_profile=Codex 解析实际恢复 Provider；
///   3. 比较解析结果与两个候选 profile。
///
/// 预期结果：
///   - 正断言：恢复 Provider 是 Codex；
///   - 负断言：恢复 Provider 不得采用旧 checkpoint 的 Claude。
#[test]
fn document_profile_is_authoritative_when_checkpoint_profile_is_stale() {
    let checkpoint = SessionCheckpoint {
        node_id: 1,
        kind: SessionCheckpointKind::Agent,
        payload: "stale Claude checkpoint".to_string(),
        cols: 80,
        rows: 24,
        shell: None,
        agent_profile: Some(AgentProfile::Claude),
    };

    let profile = crate::session_restore::resolve_agent_restore_profile(
        AgentProfile::Codex,
        Some(&checkpoint),
    );

    assert_eq!(
        profile,
        AgentProfile::Codex,
        "恢复 Provider 必须采用 Canvas document 的 Codex profile"
    );
    assert_ne!(
        profile,
        AgentProfile::Claude,
        "旧 checkpoint 的 Claude profile 不得覆盖 Canvas document 中的 Codex"
    );
}
