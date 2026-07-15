//! E2E-05：Terminal / Agent 卡片初始化动画（L1 headless GPUI）。
//!
//! 只验证可观察的 app 生命周期状态：用户新建 Terminal 会登记 Ghostty 初始化帧，
//! 并由全局动画帧循环推进。画布恢复不调用新的 spawn 路径，因此不应凭空登记该状态。

use std::time::Duration;

use gpui::{point, px, ClipboardItem, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext};

use crate::harness::{
    create_codex_agent, create_cursor_agent, create_terminal, echo_factory, enter_canvas, make_app,
};

/// E2E-05：用户新建 Terminal 后从 Ghostty 第 17 帧开始，31ms 后推进一帧。
///
/// 数据构造（含关键数值推导）：
///   - `echo_factory()` 提供确定性 PTY，避免真实 shell 输出影响初始化状态。
///   - Terminal 原始片段为上游 frame_017 至 frame_048；app 内 index=0 对应 frame_017。
///   - 初始化帧间隔为 31ms；mock clock 推进 31ms 后应从 index=0 进入 index=1。
///
/// 执行过程：
///   1. headless 启动 app、进入空 Canvas。
///   2. dispatch `NewTerminal`，读取公开的初始化动画 snapshot。
///   3. 通过 e2e 精确时钟推进 31ms，再读取一次 snapshot。
///
/// 预期结果：
///   - 正断言：新建 Terminal 有 kind="terminal"、frame_index=0 的初始化状态。
///   - 正断言：31ms 后 frame_index=1，覆盖层仍全不透明。
///   - 负断言：新建 Terminal 不应错误登记为 Agent 动画，也不应在第 0 帧就显示真实内容。
#[test]
fn e2e_05_new_terminal_starts_ghostty_intro_and_advances_after_31ms() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_terminal(&app, cx);

    let start = app.update(cx, |app, _| app.initialization_e2e_snapshot(node_id));
    let start = start.expect("new Terminal should expose its initialization animation snapshot");
    assert_eq!(
        start.kind, "terminal",
        "NewTerminal 应登记 Terminal 而不是 Agent 初始化状态"
    );
    assert_eq!(
        start.asset, "ghostty-terminal-16-47",
        "NewTerminal 应从固定的 Ghostty frame_017..048 二进制资源启动"
    );
    assert_eq!(
        start.frame_index, 0,
        "NewTerminal 应从 Ghostty frame_017（app index=0）启动"
    );
    assert_eq!(
        start.overlay_opacity_milli, 1000,
        "0ms 的 Ghostty 覆盖层应完全不透明"
    );
    assert_eq!(
        start.content_opacity_milli, 0,
        "0ms 的真实终端内容应完全被 Ghostty 覆盖层遮住"
    );

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(31), cx);
    });
    let advanced = app.update(cx, |app, _| app.initialization_e2e_snapshot(node_id));
    let advanced = advanced.expect("31ms 后初始化动画应仍在播放");
    assert_eq!(
        advanced.kind, "terminal",
        "Terminal 动画推进后仍应使用 Ghostty，而不是 Agent 资源"
    );
    assert_eq!(
        advanced.frame_index, 1,
        "31ms 恰好一帧，Ghostty 初始化动画应从 index=0 推进到 index=1"
    );
    assert_eq!(
        advanced.content_opacity_milli, 0,
        "806ms 交叉淡入开始前，真实终端内容不应提前透出"
    );
    assert_ne!(
        advanced.kind, "agent",
        "新建 Terminal 不应错误登记 Norma Agent 初始化动画"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：用户按 Enter 时初始化覆盖层在 80ms 内让真实 Terminal 接管。
///
/// 数据构造（含关键数值推导）：
///   - 新 Terminal 初始已 focus；`enter` 是无需 IME 延迟的 terminal control key。
///   - 快速退出时长固定 80ms；mock clock 精确推进 80ms 后 overlay 应被移除。
///
/// 执行过程：
///   1. 创建 Terminal，确认其初始化 snapshot 存在。
///   2. `simulate_keystrokes("enter")` 经真实 on_key_down → PTY 输入路径。
///   3. 推进 80ms，读取公开 snapshot。
///
/// 预期结果：
///   - 正断言：按键后 PTY 输入路径可用，快速退出计时器能收敛。
///   - 正断言：80ms 后 snapshot 为 None，真实内容不再被覆盖。
///   - 负断言：用户操作后不应继续停留在完整 992ms 的自然播放序列。
#[test]
fn e2e_05_terminal_input_interrupts_intro_within_80ms() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_terminal(&app, cx);
    assert!(
        app.update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
            .is_some(),
        "用户按键前新建 Terminal 应仍显示初始化覆盖层"
    );

    cx.simulate_keystrokes("enter");
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(80), cx);
    });

    assert!(
        app.update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
            .is_none(),
        "Enter 输入后 80ms 内应移除初始化覆盖层，而非继续自然播放"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：从公开 Codex Agent action 创建的卡片使用 Codex 品牌初始化资源。
///
/// 数据构造（含关键数值推导）：
///   - 使用 `NewCodexAgent` 作为公开 action；profile=Codex 决定本次使用
///     `codex-brand-ascii` 资源，而非通用 Norma ASCII 资源。
///   - 初始帧 index=0，覆盖层/内容层透明度分别应为 1000/0（毫值）。
///
/// 执行过程：
///   1. 以测试 PTY 工厂启动 app 并进入 Canvas。
///   2. dispatch `NewCodexAgent`，取新 Agent node id。
///   3. 从公开 app snapshot 读取初始化状态。
///
/// 预期结果：
///   - 正断言：Codex 新建路径登记 kind="codex"、asset="codex-brand-ascii" 的第 0 帧。
///   - 正断言：第 0 帧仍完全覆盖真实 Agent terminal。
///   - 负断言：Codex 卡片不应复用通用 Norma ASCII 或 Ghostty Terminal 资源。
#[test]
fn e2e_05_new_codex_agent_starts_codex_brand_intro() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    let snapshot = app
        .update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
        .expect("new Agent should expose its Norma initialization snapshot");
    assert_eq!(
        snapshot.kind, "codex",
        "NewCodexAgent 应登记 Codex 专属初始化资源"
    );
    assert_eq!(
        snapshot.asset, "codex-brand-ascii",
        "Codex 初始化状态必须指向由用户指定图标转化的 ASCII 资源"
    );
    assert_eq!(
        snapshot.frame_index, 0,
        "新 Agent 应从 Norma 动画第 0 帧开始"
    );
    assert_eq!(
        snapshot.overlay_opacity_milli, 1000,
        "第 0 帧应完全显示 Norma 初始化画面"
    );
    assert_eq!(
        snapshot.content_opacity_milli, 0,
        "第 0 帧不应提前透出真实 Agent terminal"
    );
    assert_ne!(
        snapshot.kind, "terminal",
        "Codex 卡片不应错误复用 Ghostty Terminal 初始化资源"
    );

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(31 * 31), cx);
    });
    let stable = app
        .update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
        .expect("Agent 最后稳定帧仍应处于 992ms 自然播放窗口内");
    assert_eq!(
        stable.asset, "codex-brand-ascii",
        "动画推进到最后一帧时仍必须保留 Codex 品牌资源，不能切换回 ASCII 帧资源"
    );
    assert_eq!(
        stable.frame_index, 31,
        "31 × 31ms 后应位于 Codex 初始化时间线的最后稳定帧"
    );
    assert_ne!(
        stable.asset, "norma-agent-duotone",
        "Codex 初始化期间不得回退到旧的 Norma ASCII 肖像资源"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：从公开 Cursor action 创建的卡片使用 Cursor 六边形箭头初始化资源。
///
/// 数据构造（含关键数值推导）：
///   `NewCursorAgent` 创建 1 张 Agent 卡片；初始帧 index=0。
///   覆盖层/内容层的初始透明度分别为 1000/0（毫值），保证品牌动画先于真实 PTY 显示。
///
/// 执行过程：
///   1. 用确定性 PTY 工厂启动 app 并进入空 Canvas。
///   2. dispatch `NewCursorAgent`，读取新节点的公开初始化动画 snapshot。
///   3. 对比 kind、资源名与透明度，验证 profile 到动画资源的完整映射。
///
/// 预期结果：
///   - 断言 A：Cursor 卡片登记 kind="cursor"、asset="cursor-brand-ascii"。
///   - 断言 B：第 0 帧覆盖层完全不透明，真实内容不提前透出。
///   - 断言 C：资源不得回退为 Codex 或通用 Norma Agent 动画。
#[test]
fn e2e_05_new_cursor_agent_starts_cursor_brand_intro() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Cursor 动画测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_cursor_agent(&app, cx);

    let snapshot = app
        .update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
        .expect("new Cursor Agent should expose its initialization snapshot");
    assert_eq!(
        snapshot.kind, "cursor",
        "NewCursorAgent 应登记 Cursor 专属初始化资源"
    );
    assert_eq!(
        snapshot.asset, "cursor-brand-ascii",
        "Cursor 初始化状态必须指向由六边形箭头图标生成的 ASCII 资源"
    );
    assert_eq!(
        snapshot.frame_index, 0,
        "新建 Cursor Agent 应从品牌动画第 0 帧开始"
    );
    assert_eq!(
        snapshot.overlay_opacity_milli, 1000,
        "Cursor 第 0 帧应完全显示初始化覆盖层"
    );
    assert_eq!(
        snapshot.content_opacity_milli, 0,
        "Cursor 第 0 帧不应提前透出真实 Agent terminal"
    );
    assert_ne!(
        snapshot.asset, "codex-brand-ascii",
        "Cursor 卡片不应错误复用 Codex 品牌动画"
    );
    assert_ne!(
        snapshot.asset, "norma-agent-duotone",
        "Cursor 卡片不应回退到通用 Norma Agent 动画"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：粘贴、IME 提交、内容点击和终端滚动都会让初始化覆盖层在 80ms 内退出。
///
/// 数据构造（含关键数值推导）：
///   四种操作均在新建 Terminal 的 0ms 覆盖层期间发生；快速退出为固定 80ms。
///   粘贴经 Cmd+V，IME 经 GPUI `simulate_input`，点击/滚动经真实 Canvas pointer
///   handler，分别覆盖四条不同的生产输入入口。
///
/// 执行过程：
///   1. 为每个输入入口创建独立 Terminal，保证前一项的 80ms 收尾不污染下一项。
///   2. 触发真实输入事件，再由 e2e 时钟推进 80ms。
///   3. 读取公开 snapshot，确认覆盖层已移除。
///
/// 预期结果：
///   - 正断言：四种用户操作均能中断初始化。
///   - 负断言：任一操作都不能让覆盖层继续播放完整 992ms。
#[test]
fn e2e_05_all_user_input_paths_interrupt_intro_within_80ms() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());

    let paste_node = create_terminal(&app, cx);
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("intro-paste".to_string()));
    });
    cx.simulate_keystrokes("cmd-v");
    assert_intro_dismisses_after_80ms(&app, cx, paste_node, "Cmd+V 粘贴");

    let ime_node = create_terminal(&app, cx);
    cx.simulate_input("初始化-IME");
    assert_intro_dismisses_after_80ms(&app, cx, ime_node, "IME 文本提交");

    let click_node = create_terminal(&app, cx);
    let click_point = terminal_content_point(&app, cx, click_node);
    cx.simulate_click(click_point, Modifiers::none());
    assert_intro_dismisses_after_80ms(&app, cx, click_node, "终端内容点击");

    let scroll_node = create_terminal(&app, cx);
    let scroll_point = terminal_content_point(&app, cx, scroll_node);
    cx.simulate_event(ScrollWheelEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(point(px(0.0), px(-80.0))),
        ..Default::default()
    });
    assert_intro_dismisses_after_80ms(&app, cx, scroll_node, "终端内容滚动");

    app.update(cx, |app, _| app.runtime.kill_all());
}

fn assert_intro_dismisses_after_80ms(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: canvas::NodeId,
    input_label: &str,
) {
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(80), cx);
    });
    assert!(
        app.update(cx, |app, _| app.initialization_e2e_snapshot(node_id))
            .is_none(),
        "{input_label} 后 80ms 内必须移除初始化覆盖层，而非继续完整 992ms 自然播放"
    );
}

fn terminal_content_point(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: canvas::NodeId,
) -> gpui::Point<gpui::Pixels> {
    app.read_with(cx, |app, _| {
        let node = app
            .runtime
            .document
            .node(node_id)
            .expect("新建 Terminal 应仍存在于画布 document");
        let center = canvas::Vec2::new(
            node.bounds.origin.x + node.bounds.size.x / 2.0,
            node.bounds.origin.y + node.bounds.size.y / 2.0,
        );
        let screen = app.runtime.document.viewport.canvas_to_screen(center);
        point(px(screen.x), px(screen.y))
    })
}
