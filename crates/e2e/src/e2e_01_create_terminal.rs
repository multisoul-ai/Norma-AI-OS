//! E2E-01：创建 Terminal 节点 + 焦点 + layout reflow（SPEC §4.2）。
//!
//! 在 headless GPUI `TestAppContext` 中通过 echo PTY 工厂装配 `MultisoulApp`，
//! 进入 Canvas 后 dispatch `NewTerminal`，对 runtime 状态做正/负断言矩阵验证。
//!
//! 本模块复用 `crate::harness` 的 helper：`make_app` / `enter_canvas` /
//! `create_terminal` / `read_runtime` / `read_grid`。所有断言只走公开 API：
//! `runtime.live_len()`、`runtime.document.nodes()`、`runtime.focus.mode()`、
//! `runtime.node_view(node_id)`、`CanvasNode.bounds.size`。

use std::time::Duration;

use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use runtime::InputMode;

use crate::harness::{
    create_terminal, drain_until, echo_factory, enter_canvas, make_app, read_grid, read_runtime,
};

/// E2E-01：`NewTerminal` 后恰好 1 个 Term 节点、焦点指向它、layout 已 reflow。
///
/// 场景：用户从 Launchpad 进入 Canvas，按 Cmd+N 创建第一个 Terminal 节点。
/// 验证整个脚手架（harness + echo PTY 注入 + screen 状态机 + spawn_terminal
/// + activate_spawned_node + apply_layout_for_current_mode）协同产出 SPEC §4.2
/// 要求的最终状态。
///
/// 数据构造（含关键数值推导）：
///   - `echo_factory()`：同步回吐 PTY 工厂。`write(bytes)` 立刻在 drain 通道
///     推 `PtyEvent::Output(bytes)`，L1 echo 一轮 12ms drain 即收敛。
///   - `tempfile::TempDir::new()`：临时 workspace root，注入
///     `Runtime::set_workspace_root` 作为 cwd。`_tmp` 持有到函数末尾以保活。
///   - 期望节点数 = 1（仅本次 `NewTerminal` 创建；进入 canvas 不带快照）。
///   - 期望焦点 mode = `InputMode::NodeFocused(node_id)`：派生自
///     `on_new_terminal` → `activate_spawned_node` → `focus.focus_node(node_id)`。
///   - 期望 bounds.size.{x,y} > 0：派生自 `apply_layout_for_current_mode` →
///     `apply_workspace_reflow_for_area` → `compute_responsive_grid_layout`，
///     单节点占据整个 `visible_canvas_rect`（去掉 layout_gap inset），宽高必为正。
///
/// 执行过程：
///   1. `make_app(cx, echo_factory())` → boot headless GPUI 窗口，root view
///      focused + window activated；初始 screen = Launchpad，runtime 已注入
///      echo 工厂（`new_with_pty_factory`）。
///   2. `enter_canvas(app, cx, tmp.path())` → `enter_canvas_for_test` 直接置
///      `screen = Canvas` 并 `set_workspace_root(root)`，不替换 runtime，
///      echo 工厂得以保留。
///   3. `create_terminal(app, cx)` → dispatch `NewTerminal`：
///        a. `on_new_terminal` 检查 `screen == Canvas`（通过）。
///        b. `runtime.spawn_terminal` 走注入工厂 → echo backend → 返回 node_id。
///        c. `activate_spawned_node`：select_only(node_id) + focus_node(node_id)。
///        d. `apply_layout_for_current_mode`：单节点 reflow 写入 document.bounds。
///      helper 内部 `run_until_parked` 收敛同步回吐与状态变更。
///   4. `drain_until(..., 200ms)` 兜底推进 12ms 周期 drain timer，确保 echo
///      回吐被 `TerminalSession` 消费（虽然 E2E-01 不验证 grid 内容，但让
///      session 进入稳态避免后续 read_grid 撞上正在 drain 的中间态）。
///
/// 预期结果：
///   - 正断言：`runtime.live_len() == 1`（恰好 1 个 live node）。
///   - 正断言：唯一节点的 `kind` 为 `CanvasNodeKind::Term(_)`。
///   - 正断言：`runtime.focus.mode() == InputMode::NodeFocused(node_id)`。
///   - 正断言：唯一节点 `bounds.size.{x,y} > 0.0`（layout 已 reflow）。
///   - 正断言：`runtime.node_view(node_id)` 为 `Some(NodeView::Term { .. })`。
///   - 负断言：不存在 `Agent` / `Preview` / `Rect` 节点（节点总数 == Term 数 == 1）。
///   - 负断言：`runtime.focus.mode() != InputMode::CanvasSelect`（焦点非空）。
///   - 负断言：节点 `bounds.size.{x,y}` 不为 0（layout 非空）。
#[test]
fn e2e_01_create_terminal_focuses_and_reflows() {
    let mut cx = TestAppContext::single();

    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());
    let node = create_terminal(&app, cx);

    // 兜底推进 12ms 周期 drain timer，让 echo 回吐被 session 消费。
    // L1 echo 通常一轮即收敛；200ms timeout 是安全冗余。
    // available drain budget = 200ms >> 12ms period → ~16 轮，足够收敛。
    drain_until(
        &app,
        cx,
        |r| {
            r.node_view(node)
                .map(|v| matches!(v, runtime::NodeView::Term { .. }))
                .unwrap_or(false)
                .then_some(())
        },
        Duration::from_millis(200),
    )
    .expect("echo backend should produce a Term node view within 200ms");

    // ---- 正断言：恰好 1 个 live node ----
    let live_len = read_runtime(&app, cx, |r| r.live_len());
    assert_eq!(
        live_len, 1,
        "create_terminal 后应恰好有 1 个 live node，实际为 {}",
        live_len
    );

    // ---- 正断言：唯一节点是 Term；负断言：无其他类型节点 ----
    let nodes = read_runtime(&app, cx, |r| r.document.nodes().to_vec());
    assert_eq!(
        nodes.len(),
        1,
        "document 应含且仅含 1 个节点（reflow 后无多余节点），实际为 {}",
        nodes.len()
    );

    let only = &nodes[0];
    assert_eq!(
        only.id, node,
        "document 唯一节点 id 应等于 create_terminal 返回的 {}，实际为 {}",
        node, only.id
    );
    assert!(
        matches!(only.kind, CanvasNodeKind::Term(_)),
        "唯一节点 kind 应为 Term(_)，实际为 {:?}",
        only.kind
    );
    assert!(
        !matches!(only.kind, CanvasNodeKind::Agent(_)),
        "不应存在 Agent 节点，但发现 {:?}",
        only.kind
    );
    assert!(
        !matches!(only.kind, CanvasNodeKind::Preview(_)),
        "不应存在 Preview 节点，但发现 {:?}",
        only.kind
    );
    assert!(
        !matches!(only.kind, CanvasNodeKind::Rect(_)),
        "不应存在 Rect 节点，但发现 {:?}",
        only.kind
    );

    // ---- 正断言：焦点指向新节点；负断言：焦点非 CanvasSelect ----
    let focus_mode = read_runtime(&app, cx, |r| r.focus.mode());
    assert_eq!(
        focus_mode,
        InputMode::NodeFocused(node),
        "activate_spawned_node 后焦点应为 NodeFocused({})，实际为 {:?}",
        node,
        focus_mode
    );
    assert_ne!(
        focus_mode,
        InputMode::CanvasSelect,
        "创建终端后焦点不应停留在 CanvasSelect（焦点不应为空）"
    );

    // ---- 正断言：layout 已 reflow（bounds 非零）；负断言：bounds 不为 0 ----
    // size.x = width, size.y = height（CanvasRect.size: Vec2）。
    let bounds = only.bounds;
    assert!(
        bounds.size.x > 0.0,
        "reflow 后节点 width 应 > 0（layout 非空），实际为 {}",
        bounds.size.x
    );
    assert!(
        bounds.size.y > 0.0,
        "reflow 后节点 height 应 > 0（layout 非空），实际为 {}",
        bounds.size.y
    );

    // ---- 正断言：Term 视图可读（NodeView::Term 存在）----
    let grid = read_grid(&app, cx, node);
    assert!(
        grid.is_some(),
        "新建节点 {} 应有 Term 视图（read_grid 返回 Some），但返回 None",
        node
    );

    // 清理：杀掉所有 live node 的 PTY（echo backend 无真实进程，但保持习惯）。
    app.update(cx, |a, _cx| a.runtime.kill_all());
}
