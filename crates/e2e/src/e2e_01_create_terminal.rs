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
///   要求的最终状态。
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
///   - reflow 验证用第二节点：`on_new_terminal` 在调用
///     `apply_layout_for_current_mode` 之前，把 spawn bounds 设为
///     `visible_canvas_rect(window).inset(layout_gap)`（`crates/app/src/app/mod.rs`
///     `on_new_terminal`）。两次 `NewTerminal` 之间窗口尺寸不变 → spawn bounds
///     的 origin 完全相同。若 reflow 被跳过，两个节点会保留同一 origin；
///     reflow 执行后 `compute_responsive_grid_layout` 把它们平铺到 2 个不同的
///     网格 cell，origin 必然不同。因此 `node1.bounds.origin != node2.bounds.origin`
///     直接证明 reflow 路径执行过。
///
/// 执行过程：
///   1. `make_app(cx, echo_factory())` → boot headless GPUI 窗口，root view
///      focused + window activated；初始 screen = Launchpad，runtime 已注入
///      echo 工厂（`new_with_pty_factory`）。
///   2. `enter_canvas(app, cx, tmp.path())` → `enter_canvas_for_test` 直接置
///      `screen = Canvas` 并 `set_workspace_root(root)`，不替换 runtime，
///      echo 工厂得以保留。
///   3. `create_terminal(app, cx)` → dispatch `NewTerminal`（第一个终端）：
///      a. `on_new_terminal` 检查 `screen == Canvas`（通过）。
///      b. `runtime.spawn_terminal` 走注入工厂 → echo backend → 返回 node_id。
///      c. `activate_spawned_node`：select_only(node_id) + focus_node(node_id)。
///      d. `apply_layout_for_current_mode`：单节点 reflow 写入 document.bounds。
///      helper 内部 `run_until_parked` 收敛同步回吐与状态变更。
///   4. 在第一节点上做 PRIMARY 1-node 断言（live_len==1、kind==Term、focus、
///      bounds 非零、Term 视图可读）—— 这是 SPEC §4.2 E2E-01 的主契约。
///   5. reflow-path 验证（专用步骤，非 E2E-03 多节点平铺覆盖）：
///      `create_terminal(app, cx)` 再创建一个终端，然后断言两个节点的
///      `bounds.origin` 不相等。若 reflow 被跳过，两次 spawn 写入的 origin
///      相同（同一 `visible_canvas_rect.inset(gap)`）；reflow 执行后必然不同。
///      此步骤仅验证“reflow 路径确实跑了”，不验证三列平铺细节（E2E-03 范畴）。
///   6. `drain_until(..., 200ms)` 兜底推进 12ms 周期 drain timer，确保 echo
///      回吐被 `TerminalSession` 消费（虽然 E2E-01 不验证 grid 内容，但让
///      session 进入稳态避免后续 read_grid 撞上正在 drain 的中间态）。
///
/// 预期结果：
///   - 正断言（PRIMARY, 1-node, 第一个终端之后）：
///     - `runtime.live_len() == 1`（恰好 1 个 live node）。
///     - 唯一节点的 `kind` 为 `CanvasNodeKind::Term(_)`。
///     - `runtime.focus.mode() == InputMode::NodeFocused(node_id)`。
///     - 唯一节点 `bounds.size.{x,y} > 0.0`（layout 非空）。
///     - `runtime.node_view(node_id)` 为 `Some(NodeView::Term { .. })`。
///   - 正断言（reflow 验证, 第二个终端之后）：
///     - 两节点 `bounds.origin` 不相等（reflow 路径确实执行）。
///     - `runtime.live_len() == 2`（第二终端已创建）。
///   - 负断言：不存在 `Agent` / `Preview` / `Rect` 节点（节点总数 == Term 数）。
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

    // ---- PRIMARY 正断言：恰好 1 个 live node ----
    let live_len = read_runtime(&app, cx, |r| r.live_len());
    assert_eq!(
        live_len, 1,
        "create_terminal 后应恰好有 1 个 live node，实际为 {}",
        live_len
    );

    // ---- PRIMARY 正断言：唯一节点是 Term；负断言：无其他类型节点 ----
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

    // ---- PRIMARY 正断言：焦点指向新节点；负断言：焦点非 CanvasSelect ----
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

    // ---- PRIMARY 正断言：layout 非空（bounds 非零）；负断言：bounds 不为 0 ----
    // 注意：此断言只验证“layout 非空”，不证明 reflow 跑过——
    // reflow 路径的证明在下面“第二终端 origin 不同”步骤。
    // size.x = width, size.y = height（CanvasRect.size: Vec2）。
    let bounds_first = only.bounds;
    assert!(
        bounds_first.size.x > 0.0,
        "reflow 后节点 width 应 > 0（layout 非空），实际为 {}",
        bounds_first.size.x
    );
    assert!(
        bounds_first.size.y > 0.0,
        "reflow 后节点 height 应 > 0（layout 非空），实际为 {}",
        bounds_first.size.y
    );

    // ---- PRIMARY 正断言：Term 视图可读（NodeView::Term 存在）----
    let grid = read_grid(&app, cx, node);
    assert!(
        grid.is_some(),
        "新建节点 {} 应有 Term 视图（read_grid 返回 Some），但返回 None",
        node
    );

    // ---- reflow-path 验证（专用步骤，非 E2E-03 多节点平铺覆盖）----
    // 创建第二个终端：若 reflow 被跳过，两次 spawn 写入的 origin 相同
    // （同一 visible_canvas_rect.inset(gap)）；reflow 执行后必然不同。
    let node2 = create_terminal(&app, cx);
    drain_until(
        &app,
        cx,
        |r| {
            r.node_view(node2)
                .map(|v| matches!(v, runtime::NodeView::Term { .. }))
                .unwrap_or(false)
                .then_some(())
        },
        Duration::from_millis(200),
    )
    .expect("echo backend should produce a Term node view for the second terminal within 200ms");

    let nodes_after = read_runtime(&app, cx, |r| r.document.nodes().to_vec());
    assert_eq!(
        nodes_after.len(),
        2,
        "第二终端创建后 document 应含 2 个节点，实际为 {}",
        nodes_after.len()
    );
    let live_len_after = read_runtime(&app, cx, |r| r.live_len());
    assert_eq!(
        live_len_after, 2,
        "第二终端创建后应恰好有 2 个 live node，实际为 {}",
        live_len_after
    );

    // 取出两个节点的当前 bounds（reflow 已对两者重新分配）。
    let b1 = nodes_after
        .iter()
        .find(|n| n.id == node)
        .expect("第一终端节点应仍存在于 document")
        .bounds;
    let b2 = nodes_after
        .iter()
        .find(|n| n.id == node2)
        .expect("第二终端节点应存在于 document")
        .bounds;

    // 核心断言：origin 不同 → reflow 路径确实执行。
    // 若 reflow 被跳过，两次 spawn 写入的 origin 相同（同一 visible_canvas_rect.inset(gap)）。
    assert_ne!(
        (b1.origin.x, b1.origin.y),
        (b2.origin.x, b2.origin.y),
        "reflow 后两节点 origin 必须不同（证明 apply_layout_for_current_mode 执行过），\
         实际 node1=({:?}, {:?}) node2=({:?}, {:?})",
        b1.origin.x,
        b1.origin.y,
        b2.origin.x,
        b2.origin.y
    );

    // 清理：杀掉所有 live node 的 PTY（echo backend 无真实进程，但保持习惯）。
    app.update(cx, |a, _cx| a.runtime.kill_all());
}
