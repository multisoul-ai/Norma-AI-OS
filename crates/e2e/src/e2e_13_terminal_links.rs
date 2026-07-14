//! E2E-13：终端（含 Agent PTY）输出网页链接后，点击在 Norma 内打开新的 Browser master 卡片。

use std::time::Duration;

use canvas::{CanvasNodeKind, NodeId, Vec2};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};

use crate::harness::{
    create_terminal, drain_until, echo_factory, enter_canvas, make_app, read_runtime,
};

/// E2E-13：点击终端中的裸 https URL 后，新 Browser 成为 master，原终端留在 detail，且不抢网页键盘焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   Terminal 输出 = "https://example.com/docs"，首字符位于 row=0,col=0
///   terminal 内容首格屏幕坐标 = node top-left + padding_x(8) + 半 cell(4)，
///                              y = node top-left + title(26) + padding_y(8) + 半 line(8)
///   先前 Browser 数 = 0；链接点击后期望 Browser 数 = 1
///   master/detail 宽度分配中 master 比 detail 宽，因此 `browser.width > terminal.width`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用 echo PTY 进入 Canvas 并创建 Terminal，向其输入裸 https URL 后等待 grid 收到回显。
///   2. 点击 URL 首个字符的真实 Canvas 坐标；生产鼠标路由查询 terminal link intent。
///   3. app 新建 Browser、选中它、仅在链接打开路径进入 master/detail，并把 Browser 设为 master。
///
/// 预期结果：
///   - 断言 A：存在且仅存在一张 Browser，保存 URL 与终端输出精确一致。
///   - 断言 B：Browser 是唯一 selected 节点，证明链接打开后未把键盘焦点直接交给网页。
///   - 断言 C：Browser 位于原 Terminal 左侧且更宽，证明它成为 master。
///   - 断言 D：原 Terminal 仍存在且没有被替换，证明点击只新增 Browser。
#[test]
fn e2e_13_terminal_https_link_creates_selected_browser_master_card() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 terminal link e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let terminal_id = create_terminal(&app, cx);
    let url = "https://example.com/docs";

    cx.simulate_input(url);
    let grid_ready = drain_until(
        &app,
        cx,
        |runtime| {
            runtime.node_view(terminal_id).and_then(|view| match view {
                runtime::NodeView::Term { snapshot, .. } => snapshot
                    .link_at(0, 0)
                    .as_deref()
                    .filter(|candidate| *candidate == url)
                    .map(|_| ()),
            })
        },
        Duration::from_millis(200),
    );
    assert_eq!(
        grid_ready,
        Some(()),
        "echo PTY 回显后的 terminal 首格应可识别为可点击 https URL"
    );

    let click = terminal_first_cell_point(&app, cx, terminal_id);
    cx.simulate_click(click, Modifiers::none());
    cx.run_until_parked();

    let (browsers, terminal, selected) = read_runtime(&app, cx, |runtime| {
        let browsers = runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| match &node.kind {
                CanvasNodeKind::Browser(spec) => Some((node.id, spec.url.clone(), node.bounds)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let terminal = runtime
            .document
            .node(terminal_id)
            .expect("链接打开不应删除原 Terminal")
            .bounds;
        let selected = runtime.selection.iter().collect::<Vec<_>>();
        (browsers, terminal, selected)
    });

    assert_eq!(
        browsers.len(),
        1,
        "点击一次链接应只创建一张 Browser，不能重复创建或不创建"
    );
    let (browser_id, browser_url, browser_bounds) = &browsers[0];
    assert_eq!(
        browser_url, url,
        "新 Browser 必须加载链接的精确 URL，不能退回默认主页"
    );
    assert_eq!(
        selected,
        vec![*browser_id],
        "新 Browser 应是唯一 selected 节点，但不应自动把网页设为键盘焦点"
    );
    assert!(
        browser_bounds.origin.x < terminal.origin.x,
        "Browser master 应位于原 Terminal detail 左侧"
    );
    assert!(
        browser_bounds.size.x > terminal.size.x,
        "Browser master 宽度应大于 Terminal detail，证明 master/detail 布局已应用"
    );
}

/// E2E-13：从终端链接拖出文字选择时，不得把按下动作误判为打开页面。
///
/// 数据构造（含关键数值的推导过程）：
///   Terminal 输出 = "https://example.com/docs"，首字符在 row=0,col=0
///   drag 起点 = URL 首格中心；终点 x = 起点 + 3.5px，仍在同一 8px 宽 cell 内
///   拖动前 Browser 数 = 0；拖动后期望仍为 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 echo Terminal，并等待首格可识别为 https 链接。
///   2. 在链接首格发送 mouse down，再带 Left pressed_button 在同一 cell 内移动 3.5px，最后 mouse up。
///   3. 生产路由把 pending link 改为文本选择，mouse up 不再消费为 Browser 打开请求。
///
/// 预期结果：
///   - 断言 A：Browser 数仍为 0，证明拖拽不打开页面。
///   - 断言 B：原 Terminal 仍存在，证明拖拽不会改变卡片集合。
#[test]
fn e2e_13_terminal_link_drag_selects_text_without_opening_browser() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 terminal drag e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let terminal_id = create_terminal(&app, cx);
    let url = "https://example.com/docs";

    cx.simulate_input(url);
    let grid_ready = drain_until(
        &app,
        cx,
        |runtime| {
            runtime.node_view(terminal_id).and_then(|view| match view {
                runtime::NodeView::Term { snapshot, .. } => snapshot
                    .link_at(0, 0)
                    .as_deref()
                    .filter(|candidate| *candidate == url)
                    .map(|_| ()),
            })
        },
        Duration::from_millis(200),
    );
    assert_eq!(
        grid_ready,
        Some(()),
        "拖拽前 terminal 首格应已包含可点击 https URL"
    );

    let start = terminal_first_cell_point(&app, cx, terminal_id);
    let end = point(start.x + px(3.5), start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let (browser_count, terminal_exists) = read_runtime(&app, cx, |runtime| {
        (
            runtime
                .document
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
                .count(),
            runtime.document.node(terminal_id).is_some(),
        )
    });
    assert_eq!(
        browser_count, 0,
        "拖拽 URL 只能开始文本选择，不能创建 Browser 卡片"
    );
    assert!(
        terminal_exists,
        "拖拽 URL 后原 Terminal 必须仍存在，不能被链接打开流程替换"
    );
}

fn terminal_first_cell_point(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: NodeId,
) -> gpui::Point<gpui::Pixels> {
    app.read_with(cx, |app, _| {
        let node = app
            .runtime
            .document
            .node(node_id)
            .expect("Terminal 节点必须仍在 document 中");
        let zoom = app.runtime.document.viewport.zoom;
        let top_left = app
            .runtime
            .document
            .viewport
            .canvas_to_screen(node.bounds.origin);
        // x = padding(8) + half cell(4)；y = title(26) + padding(8) + half line(8)。
        let cell_center = Vec2::new(top_left.x + 12.0 * zoom, top_left.y + 42.0 * zoom);
        point(px(cell_center.x), px(cell_center.y))
    })
}
