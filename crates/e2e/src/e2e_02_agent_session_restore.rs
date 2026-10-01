//! Agent 会话恢复 L1：旧库缺失 session id 时，打开工作空间仍应恢复 Provider 历史会话。

use std::time::Duration;

use gpui::TestAppContext;

use crate::harness::{
    create_codex_agent, drain_until, echo_factory, enter_canvas, make_app, make_app_real,
    project_store,
};

#[path = "e2e_02_agent_session_restore_support.rs"]
mod support;
use support::{
    prepare_claude_history, prepare_fake_claude, prepare_multi_persisted_agents,
    prepare_persisted_agent, ProcessEnvGuard,
};

/// 旧项目数据库没有 hook session id 时，打开工作空间应发现 Claude 历史并恢复原会话。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id = 7，profile = Claude，checkpoint grid = 80×24
///   node_sessions[node_id=7].session_id = None，模拟回归前旧库或 Hook 漏报
///   Claude history = `.claude/projects/<encoded-root>/claude-history-session.jsonl`
///   fake Claude 输出 = `RESTORED_SESSION:<第三个参数>`；resume 命令第三个参数应为 session id
///   timeout = 2s；GPUI 每轮推进 12ms，理论最多约 2000/12 ≈ 166 轮
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入包含 Claude Agent 卡片的 canvas snapshot、checkpoint 和空 session id
///   2. 在临时 HOME 创建匹配工作空间的 Claude 历史文件，并用 fake CLI 捕获启动参数
///   3. 通过 App 的真实 workspace open 编排加载卡片和持久化记录
///   4. 延迟恢复在后台发现历史 session，前台生成 `claude --resume` 并启动 fake CLI
///   5. 推进 GPUI drain，读取 Agent 卡片 grid 中的 fake CLI 输出
///   6. 推进 4×120ms=480ms 持久化时钟，读取 NodeSessionStore
///
/// 预期结果：
///   - 正断言：卡片输出包含 `RESTORED_SESSION:claude-history-session`
///   - 正断言：Agent live node 成功恢复，不停留在静态 snapshot
///   - 正断言：发现的 session 在 360ms 防抖后写回 node 7
///   - 负断言：输出不应是 `RESTORED_SESSION:` 空值，否则仍启动了全新 Session
#[test]
fn e2e_02_workspace_open_restores_agent_session_from_provider_history() {
    let temp = tempfile::tempdir().expect("Agent session restore E2E 应创建临时根目录");
    let workspace = temp.path().join("workspace");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&workspace).expect("E2E workspace 目录应创建成功");
    std::fs::create_dir_all(&home).expect("E2E HOME 目录应创建成功");
    std::fs::create_dir_all(&bin).expect("E2E fake bin 目录应创建成功");
    let workspace = workspace
        .canonicalize()
        .expect("E2E workspace 应规范化为与 App 打开流程相同的 root");
    prepare_claude_history(&workspace, &home);
    prepare_fake_claude(&bin);
    let _env = ProcessEnvGuard::set(&home, &bin);

    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut cx);
    prepare_persisted_agent(project_store(&app, cx, &workspace));
    app.update(cx, |app, cx| {
        app.open_workspace_for_e2e(&workspace, cx)
            .expect("E2E 应通过真实 workspace open 编排打开项目");
    });

    let restored_text = drain_until(
        &app,
        cx,
        |runtime| {
            let runtime::NodeView::Term { snapshot, .. } = runtime.node_view(7)?;
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            text.contains("RESTORED_SESSION:").then_some(text)
        },
        Duration::from_secs(2),
    )
    .expect("2 秒内应启动 fake Claude 并在 Agent 卡片 grid 输出恢复的 session id");

    assert!(
        restored_text.contains("RESTORED_SESSION:claude-history-session"),
        "Workspace 打开后 Agent 卡片应恢复 Provider 历史 session，实际 grid: {restored_text:?}"
    );
    assert!(
        app.read_with(cx, |app, _cx| app.runtime.node_view(7).is_some()),
        "node_id=7 的 Agent live node 应完成恢复，不应只剩静态 canvas snapshot"
    );
    assert!(
        restored_text.trim() != "RESTORED_SESSION:",
        "fake Claude 不应收到空 session id，否则说明恢复路径仍启动了全新 Session，实际 grid: {restored_text:?}"
    );
    // available debounce time = 4 × persistence tick(120ms) = 480ms ≥ 360ms。
    for _ in 0..4 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(120));
        cx.run_until_parked();
    }
    let persisted = project_store(&app, cx, &workspace)
        .node_session_store()
        .load_sessions()
        .expect("历史发现后的 node session 应可读取");
    let persisted_agent = persisted
        .iter()
        .find(|session| session.node_id == 7)
        .expect("node 7 的历史发现结果应在防抖后持久化");
    assert_eq!(
        persisted_agent.session_id.as_deref(),
        Some("claude-history-session"),
        "单卡 legacy discovery 成功认领后必须持久化，避免下次打开再次猜 latest"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// 两张历史 Agent 卡片都缺失绑定时，打开工作空间不得重复使用同一个 latest session。
///
/// 数据构造（含关键数值的推导过程）：
///   Claude Agent = 2 张，node_id=7/8；checkpoint = 2 条；
///   node_sessions = 2 条，session_id 都为 None；
///   Provider history = 唯一一条 `claude-history-session`；
///   fake Claude 每个进程输出一次 `RESTORED_SESSION:<第三个参数>`；
///   timeout = 2s，GPUI 每轮约 12ms，最多约 2000/12 ≈ 166 轮。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入两张 Claude 卡片、空 node_sessions 及一条 workspace history；
///   2. 通过真实 workspace open 编排加载持久化绑定并延迟启动两个 PTY；
///   3. fake Claude 分别打印收到的 resume 参数，测试读取两张卡片的 grid。
///
/// 预期结果：
///   - 正断言：node 7 和 node 8 都成功启动独立 live Agent；
///   - 负断言：两张卡片都不得收到 `claude-history-session`；
///   - 负断言：Runtime 不得把 workspace latest session 绑定给任意一张卡片。
#[test]
fn e2e_02_workspace_open_never_reuses_latest_session_for_multiple_unbound_agent_cards() {
    let temp = tempfile::tempdir().expect("多 Agent session 冲突 E2E 应创建临时根目录");
    let workspace = temp.path().join("workspace");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&workspace).expect("多 Agent E2E workspace 目录应创建成功");
    std::fs::create_dir_all(&home).expect("多 Agent E2E HOME 目录应创建成功");
    std::fs::create_dir_all(&bin).expect("多 Agent E2E fake bin 目录应创建成功");
    let workspace = workspace
        .canonicalize()
        .expect("多 Agent E2E workspace 应规范化为真实 root");
    prepare_claude_history(&workspace, &home);
    prepare_fake_claude(&bin);
    let _env = ProcessEnvGuard::set(&home, &bin);

    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut cx);
    prepare_multi_persisted_agents(project_store(&app, cx, &workspace), [None, None]);
    app.update(cx, |app, cx| {
        app.open_workspace_for_e2e(&workspace, cx)
            .expect("多 Agent E2E 应通过真实 workspace open 打开项目");
    });

    let restored = drain_until(
        &app,
        cx,
        |runtime| {
            let runtime::NodeView::Term {
                snapshot: first, ..
            } = runtime.node_view(7)?;
            let runtime::NodeView::Term {
                snapshot: second, ..
            } = runtime.node_view(8)?;
            let first_text = first
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>();
            let second_text = second
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>();
            (first_text.contains("RESTORED_SESSION:") && second_text.contains("RESTORED_SESSION:"))
                .then_some((first_text, second_text))
        },
        Duration::from_secs(2),
    )
    .unwrap_or_else(|| {
        let screens = app.read_with(cx, |app, _| {
            [7, 8].map(|node| {
                (
                    node,
                    app.runtime.node_view(node).map(|view| match view {
                        runtime::NodeView::Term {
                            snapshot, status, ..
                        } => {
                            let text = snapshot
                                .rows
                                .iter()
                                .flatten()
                                .map(|cell| cell.c)
                                .collect::<String>();
                            (status, text)
                        }
                    }),
                )
            })
        });
        panic!("2 秒内两张 Agent 卡片都应启动 fake Claude，实际终端：{screens:?}");
    });

    assert!(
        !restored.0.contains("claude-history-session"),
        "node 7 不得与 node 8 共享 workspace latest session，实际 grid: {:?}",
        restored.0
    );
    assert!(
        !restored.1.contains("claude-history-session"),
        "node 8 不得与 node 7 共享 workspace latest session，实际 grid: {:?}",
        restored.1
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.runtime.agent_session_id(7)),
        None,
        "workspace latest session 不得被任意分配给 node 7"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.runtime.agent_session_id(8)),
        None,
        "workspace latest session 不得被任意分配给 node 8"
    );
    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// 两张卡保存了不同 session 时，workspace open 必须分别恢复各自历史。
///
/// 数据构造（含关键数值的推导过程）：
///   Claude Agent = 2 张；node 7→session-a，node 8→session-b；
///   fake CLI 输出收到的 resume 参数；timeout=2s，最多约 166 次 12ms drain。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保存两张卡及两条唯一 node session；
///   2. 通过真实 workspace open 延迟恢复两个 Agent PTY；
///   3. 分别读取 node 7/8 grid 和 Runtime 绑定。
///
/// 预期结果：
///   - 正断言：node 7 恢复 session-a，node 8 恢复 session-b；
///   - 负断言：任一节点都不得收到另一节点的 session。
#[test]
fn e2e_02_workspace_open_restores_distinct_session_for_each_agent_card() {
    let temp = tempfile::tempdir().expect("独立 Agent session E2E 应创建临时根目录");
    let workspace = temp.path().join("workspace");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    for path in [&workspace, &home, &bin] {
        std::fs::create_dir_all(path).expect("独立 Agent session E2E 目录应创建成功");
    }
    let workspace = workspace.canonicalize().expect("E2E workspace 应规范化");
    prepare_fake_claude(&bin);
    let _env = ProcessEnvGuard::set(&home, &bin);
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut cx);
    prepare_multi_persisted_agents(
        project_store(&app, cx, &workspace),
        [Some("session-a"), Some("session-b")],
    );
    app.update(cx, |app, cx| {
        app.open_workspace_for_e2e(&workspace, cx)
            .expect("独立 Agent session E2E 应打开项目");
    });

    let restored = drain_until(
        &app,
        cx,
        |runtime| {
            let runtime::NodeView::Term { snapshot: a, .. } = runtime.node_view(7)?;
            let runtime::NodeView::Term { snapshot: b, .. } = runtime.node_view(8)?;
            let a = a
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>();
            let b = b
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>();
            (a.contains("RESTORED_SESSION:") && b.contains("RESTORED_SESSION:")).then_some((a, b))
        },
        Duration::from_secs(2),
    )
    .expect("2 秒内两张 Agent 卡片都应完成独立恢复");

    assert!(
        restored.0.contains("RESTORED_SESSION:session-a"),
        "node 7 应恢复 session-a，实际: {:?}",
        restored.0
    );
    assert!(
        !restored.0.contains("session-b"),
        "node 7 不得收到 node 8 的 session-b"
    );
    assert!(
        restored.1.contains("RESTORED_SESSION:session-b"),
        "node 8 应恢复 session-b，实际: {:?}",
        restored.1
    );
    assert!(
        !restored.1.contains("session-a"),
        "node 8 不得收到 node 7 的 session-a"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.runtime.agent_session_id(7))
            .as_deref(),
        Some("session-a"),
        "Runtime 中 node 7 应绑定 session-a"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.runtime.agent_session_id(8))
            .as_deref(),
        Some("session-b"),
        "Runtime 中 node 8 应绑定 session-b"
    );
    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// Hook 只更新 session 绑定时，防抖持久化必须保存最新 ID，而不是停留在旧值。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent = 1 张；SessionStart = A、B 两次，task state 都为 Ready；
///   persistence tick = 120ms，debounce = 360ms；每轮推进 4×120=480ms ≥ 360ms。
///   Hook consume timeout = 500ms；runtime tick=16ms，最多约 500/16≈31 次消费机会。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Codex 卡片并注入 SessionStart(session-a)；
///   2. 推进 runtime 与 persistence loop，确认 session-a 已写入；
///   3. 保持 Ready，注入 SessionStart(session-b)；
///   4. 再推进 480ms，并通过 NodeSessionStore 公开接口读取结果。
///
/// 预期结果：
///   - 正断言：第一次防抖保存写入 session-a；
///   - 正断言：第二次纯绑定变化覆盖为 session-b；
///   - 负断言：最终记录不得仍是 session-a 或空值。
#[test]
fn e2e_02_session_only_hook_change_is_persisted_after_debounce() {
    let temp = tempfile::tempdir().expect("Hook session 持久化 E2E 应创建 workspace");
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    enter_canvas(&app, cx, temp.path());
    let node_id = create_codex_agent(&app, cx);
    let store = project_store(&app, cx, temp.path()).node_session_store();

    let event_a = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "session-a"
        }),
    )
    .expect("Codex SessionStart(session-a) E2E 事件应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(event_a)
            .expect("SessionStart(session-a) 应进入生产 Hook 队列");
    });
    drain_until(
        &app,
        cx,
        |runtime| (runtime.agent_session_id(node_id).as_deref() == Some("session-a")).then_some(()),
        Duration::from_millis(500),
    )
    .expect("500ms 内 Runtime 应消费 SessionStart(session-a)");
    for _ in 0..4 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(120));
        cx.run_until_parked();
    }
    let saved_a = store
        .load_sessions()
        .expect("第一次防抖后 node sessions 应可读取");
    let agent_a = saved_a
        .iter()
        .find(|session| session.node_id == node_id)
        .expect("第一次防抖后目标 Agent node session 应存在");
    assert_eq!(
        agent_a.session_id.as_deref(),
        Some("session-a"),
        "第一次 SessionStart 后必须明确持久化 session-a"
    );

    let event_b = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "session-b"
        }),
    )
    .expect("Codex SessionStart(session-b) E2E 事件应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(event_b)
            .expect("SessionStart(session-b) 应进入生产 Hook 队列");
    });
    drain_until(
        &app,
        cx,
        |runtime| (runtime.agent_session_id(node_id).as_deref() == Some("session-b")).then_some(()),
        Duration::from_millis(500),
    )
    .expect("500ms 内 Runtime 应消费 SessionStart(session-b)");
    for _ in 0..4 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(120));
        cx.run_until_parked();
    }
    let saved_b = store
        .load_sessions()
        .expect("第二次防抖后 node sessions 应可读取");
    let agent_b = saved_b
        .iter()
        .find(|session| session.node_id == node_id)
        .expect("第二次防抖后目标 Agent node session 应存在");
    assert_eq!(
        agent_b.session_id.as_deref(),
        Some("session-b"),
        "第二次纯绑定变化必须明确覆盖并持久化 session-b"
    );

    let final_saved = store.load_sessions().expect("最终 node sessions 应可读取");
    let final_agent = final_saved
        .iter()
        .find(|session| session.node_id == node_id)
        .expect("最终目标 Agent node session 应存在");
    assert_ne!(
        final_agent.session_id.as_deref(),
        Some("session-a"),
        "第二次纯绑定更新后不得继续保存旧 session-a"
    );
    assert!(
        final_agent.session_id.is_some(),
        "Hook 已提供 session-b，最终持久化值不得为空"
    );
    app.update(cx, |app, _cx| app.runtime.kill_all());
}
