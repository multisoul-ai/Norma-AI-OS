use super::*;
use canvas::CanvasRect;

/// 场景：启动静默超时且没有任务态时，卡片标题栏显示「启动较慢」警告徽标。
///
/// 数据构造：
///   state = None（Agent 尚未经 hook 上报任何任务态）、progress = None、preparing = false
///   startup_stalled = true（runtime 看门狗已判定 15s 未收到启动 hook）
///
/// 执行过程：
///   1. 调用 chrome_task_badge(None, None, false, true)
///
/// 预期结果：
///   - 正断言：返回 Some 徽标，tone 为 Warning，label 等于 i18n 文案 cards.agent.startup_slow
///   - 负断言：label 不为空，也不是任务态词汇「待开始」
#[test]
fn stalled_startup_without_task_state_shows_warning_badge() {
    let badge =
        chrome_task_badge(None, None, false, true).expect("无任务态且启动卡住时应显示启动较慢徽标");

    assert_eq!(
        badge.label,
        ui::i18n::text("cards.agent.startup_slow"),
        "徽标文案应来自 cards.agent.startup_slow"
    );
    assert!(!badge.label.is_empty(), "徽标文案不得为空");
    assert_ne!(badge.label, "待开始", "启动较慢不应冒充任务态词汇");
    assert_eq!(
        badge.tone,
        CardChromeTaskTone::Warning,
        "启动较慢应使用 Warning 语义色"
    );
}

/// 场景：启动未卡住且无任务态时不显示任何徽标。
///
/// 数据构造：state = None、startup_stalled = false
/// 执行过程：调用 chrome_task_badge(None, None, false, false)
/// 预期结果：
///   - 负断言：返回 None，健康启动的卡片标题栏不应出现任何提示
#[test]
fn healthy_startup_without_task_state_shows_no_badge() {
    let badge = chrome_task_badge(None, None, false, false);

    assert!(badge.is_none(), "启动正常且无任务态时不应出现徽标");
}

/// 场景：已有任务态时任务态徽标优先，不被启动提示覆盖。
///
/// 数据构造：state = Thinking（hook 已上报，说明 CLI 已启动）、startup_stalled = true
/// 执行过程：调用 chrome_task_badge(Some(Thinking), None, false, true)
/// 预期结果：
///   - 正断言：返回的徽标 tone 为 Active（运行中）
///   - 负断言：label 不等于 cards.agent.startup_slow
#[test]
fn task_state_badge_takes_priority_over_startup_hint() {
    let badge = chrome_task_badge(Some(AgentTaskState::Thinking), None, false, true)
        .expect("有任务态时应渲染任务态徽标");

    assert_eq!(
        badge.tone,
        CardChromeTaskTone::Active,
        "任务态徽标应保持运行中语义色"
    );
    assert_ne!(
        badge.label,
        ui::i18n::text("cards.agent.startup_slow"),
        "启动提示不得覆盖任务态徽标"
    );
}

/// 场景：静默卡住的 Agent 卡片经 `chrome()` 端到端显示徽标，Terminal 卡片不显示。
///
/// 数据构造：
///   注入 echo 假 PTY 的 Runtime，配置 hook 通道并声明 Codex 的 hook 已安装（满足武装条件）；
///   Agent(Codex) 与 Terminal 各 1 张，均不发送任何 hook；
///   tick 时刻 = 现在 + 16s（15s 限值 + 1s 余量），两个节点均越过阈值。
///
/// 执行过程：
///   1. 启动 Agent 与 Terminal 节点
///   2. tick_startup_stalls(now + 16s) → 两个节点都进入 stalled
///   3. 分别读取 Agent 卡片与 Terminal 卡片的 chrome
///
/// 预期结果：
///   - 正断言：runtime 层 Agent stalled
///   - 负断言：runtime 层 Terminal 不 stalled（看门狗只服务 Agent 节点）
///   - 正断言：Agent 卡片 task_badge 为 Warning 的「启动较慢」
///   - 负断言：Terminal 卡片 task_badge 为 None，普通终端不得出现 Agent 启动提示
#[test]
fn stalled_agent_card_shows_badge_but_terminal_card_does_not() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let mut runtime = Runtime::new_for_workspace(temp.path().to_path_buf());
    runtime.set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok".to_string(),
        workspace_id: "w".to_string(),
        workspace_route_id: "r".to_string(),
        norma_socket: "/tmp/s.sock".to_string(),
    }));
    runtime.set_startup_watch_profiles([canvas::AgentProfile::Codex]);
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    }));
    let bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let agent_id = runtime
        .spawn_agent(canvas::AgentProfile::Codex, bounds, 80, 24)
        .expect("应启动静默 Agent 卡片");
    let term_id = runtime
        .spawn_terminal(bounds, runtime::DEFAULT_TERMINAL_SHELL, 80, 24)
        .expect("应启动静默 Terminal 卡片");

    let crossed =
        runtime.tick_startup_stalls(std::time::Instant::now() + std::time::Duration::from_secs(16));

    assert!(crossed, "Agent 越过 15s 阈值，tick 应返回 true");
    assert!(
        runtime.agent_startup_stalled(agent_id),
        "Agent 节点 15s 内未收到 hook 应 stalled"
    );
    assert!(
        !runtime.agent_startup_stalled(term_id),
        "Terminal 节点不属于 Agent，不得被看门狗标记 stalled"
    );
    let agent_node = runtime.document.node(agent_id).expect("Agent 卡片应存在");
    let agent_badge = CARD
        .chrome(agent_node, &runtime)
        .task_badge
        .expect("stalled 的 Agent 卡片应显示徽标");
    assert_eq!(
        agent_badge.tone,
        CardChromeTaskTone::Warning,
        "Agent 启动提示应为 Warning"
    );
    let term_node = runtime.document.node(term_id).expect("Terminal 卡片应存在");
    let term_chrome = crate::cards::term::card::CARD.chrome(term_node, &runtime);
    assert!(
        term_chrome.task_badge.is_none(),
        "Terminal 卡片不得显示 Agent 启动徽标"
    );
}
