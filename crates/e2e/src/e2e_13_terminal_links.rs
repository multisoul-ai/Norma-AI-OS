//! E2E-13：终端（含 Agent PTY）输出网页或本机文件链接后，点击创建 Browser 或 Preview 卡片。

use std::time::Duration;

use canvas::{CanvasNodeKind, NodeId, Vec2};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};

use crate::harness::{
    create_codex_agent, create_terminal, drain_until, echo_factory, enter_canvas, make_app,
    read_runtime,
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

/// E2E-13 Preview：点击 Agent 输出的工作区外 file URI 后，应创建指向真实本机文件的 Preview。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = 独立 tempdir A
///   external dir = `/tmp` 下独立 tempdir B，明确不在 workspace A 内
///   图片 = 有效 Codex brand PNG fixture，复制为 "external output.png"
///   file URI = `file://` + 绝对路径，其中空格编码为 `%20`
///   点击位置 = Agent 内容区 row 0,col 0 的首格中心
///   点击前 Preview 数 = 0；点击后期望 Preview 数 = 1、Browser 数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用 echo PTY 创建 Codex Agent，把工作区外 PNG 的 file URI 写入并等待 grid 回显。
///   2. 点击 URI 首格；生产路由识别 Preview 目标并解析 percent-encoding。
///   3. 创建 Preview node，异步读取真实绝对路径，并保持原 Agent 卡片存在。
///
/// 预期结果：
///   - 正断言：唯一 Preview 的 spec.path 精确等于工作区外 PNG 的真实绝对路径。
///   - 正断言：新 Preview 是唯一 selected 节点，原 Agent 仍存在。
///   - 负断言：spec.path 不得保留 `file://`，也不得错误拼到 workspace 下。
///   - 负断言：本机文件不得误建 Browser 卡片。
#[test]
fn e2e_13_agent_external_file_uri_creates_local_preview_card() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Agent Preview workspace");
    let external = tempfile::Builder::new()
        .prefix("norma-preview-")
        .tempdir_in("/tmp")
        .expect("应在 /tmp 创建工作区外的 Preview fixture 目录");
    let image_path = external.path().join("external output.png");
    std::fs::write(
        &image_path,
        include_bytes!("../../app/assets/init-animation/source/codex-brand.png"),
    )
    .expect("应写入有效 PNG 作为工作区外 Preview fixture");
    let file_uri = format!("file://{}", image_path.display()).replace(' ', "%20");
    enter_canvas(&app, cx, workspace.path());
    let agent_id = create_codex_agent(&app, cx);

    cx.simulate_input(&file_uri);
    let grid_ready = drain_until(
        &app,
        cx,
        |runtime| {
            runtime.node_view(agent_id).and_then(|view| match view {
                runtime::NodeView::Term { snapshot, .. } => snapshot
                    .preview_path_at(0, 0)
                    .as_deref()
                    .filter(|candidate| *candidate == file_uri)
                    .map(|_| ()),
            })
        },
        // available wall time = 200ms；L1 echo 通常 1 个 12ms drain step 即收敛，
        // 额外 188ms = 15 个以上 step，用于吸收测试调度抖动。
        Duration::from_millis(200),
    );
    assert_eq!(
        grid_ready,
        Some(()),
        "Agent echo 后首格应识别完整的工作区外 file URI"
    );

    let click = terminal_first_cell_point(&app, cx, agent_id);
    cx.simulate_click(click, Modifiers::none());
    cx.run_until_parked();

    let (previews, browser_count, agent_exists, selected) = read_runtime(&app, cx, |runtime| {
        let previews = runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| match &node.kind {
                CanvasNodeKind::Preview(spec) => {
                    Some((node.id, std::path::PathBuf::from(&spec.path)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let browser_count = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .count();
        let agent_exists = runtime.document.node(agent_id).is_some();
        let selected = runtime.selection.iter().collect::<Vec<_>>();
        (previews, browser_count, agent_exists, selected)
    });

    assert_eq!(
        previews.len(),
        1,
        "点击一次工作区外 file URI 应只创建一张 Preview"
    );
    let (preview_id, preview_path) = &previews[0];
    assert_eq!(
        preview_path, &image_path,
        "Preview spec.path 应是 percent-decoding 后的真实工作区外绝对路径"
    );
    assert!(
        !preview_path.starts_with(workspace.path()),
        "工作区外图片不得被错误拼接到 workspace root"
    );
    assert!(
        !preview_path.to_string_lossy().starts_with("file://"),
        "Preview spec.path 不得保留无法被 PathBuf 读取的 file URI scheme"
    );
    assert_eq!(
        selected,
        vec![*preview_id],
        "file URI 创建的新 Preview 应成为唯一 selected 节点"
    );
    assert!(agent_exists, "打开 Preview 后原 Agent 卡片必须继续存在");
    assert_eq!(browser_count, 0, "本机 file URI 不得误建 Browser 卡片");
}

/// E2E-13 Wrapped Preview：点击 Agent 列表中被终端软折行的本机路径，应创建完整 Preview。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = 独立 tempdir
///   Agent grid cols = 创建卡片后从生产 Runtime snapshot 读取
///   output prefix = "- "，占 2 cells；路径由固定短目录段扩展到 path length > grid cols
///   因 output length = 2 + path length > grid cols，至少发生 1 次终端软折行
///   点击位置 = row 0,col 2，即列表项中路径首字符 `d`
///   点击前 Preview 数 = 0；点击后期望 Preview 数 = 1、Browser 数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Codex Agent，按其真实 grid 宽度构造并写入一个存在的长 PNG 相对路径。
///   2. 等待 echo PTY 把列表项写入 grid，并确认路径跨软折行后仍可从首段命中。
///   3. 点击列表项路径首格，走生产 mouse route → Preview path resolve → card spawn 链路。
///
/// 预期结果：
///   - 正断言：唯一 Preview 的 spec.path 等于 workspace 下的完整长路径。
///   - 正断言：新 Preview 成为唯一 selected，原 Agent 继续存在。
///   - 负断言：不得只使用第一视觉行的残片，也不得误建 Browser。
#[test]
fn e2e_13_agent_soft_wrapped_local_path_creates_complete_preview_card() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Agent wrapped Preview workspace");
    enter_canvas(&app, cx, workspace.path());
    let agent_id = create_codex_agent(&app, cx);
    let grid_cols = read_runtime(&app, cx, |runtime| {
        runtime.node_view(agent_id).map(|view| match view {
            runtime::NodeView::Term { snapshot, .. } => snapshot.cols,
        })
    })
    .expect("新 Agent 应暴露 terminal grid snapshot");
    let file_name = "prototype-a-floating-voice-draft.png";
    let mut relative_dir = "docs/design-docs/assets/voice-input-overflow".to_string();
    // 每轮新增 "/overflow-segment" = 17 cells；循环到
    // relative_dir + "/"(1) + file_name 的总长严格大于 grid_cols，保证软折行。
    while relative_dir.len() + 1 + file_name.len() <= grid_cols {
        relative_dir.push_str("/overflow-segment");
    }
    let relative_path = format!("{relative_dir}/{file_name}");
    let absolute_path = workspace.path().join(&relative_path);
    std::fs::create_dir_all(
        absolute_path
            .parent()
            .expect("wrapped Preview fixture 应有父目录"),
    )
    .expect("应创建 wrapped Preview fixture 目录");
    std::fs::write(
        &absolute_path,
        include_bytes!("../../app/assets/init-animation/source/codex-brand.png"),
    )
    .expect("应写入有效 PNG 作为 wrapped Preview fixture");
    let output = format!("- {relative_path}");

    assert!(
        relative_path.len() > grid_cols,
        "构造出的相对路径必须长于 Agent grid，才能确定性覆盖软折行；path={} cols={grid_cols}",
        relative_path.len()
    );
    cx.simulate_input(&output);
    let grid_ready = drain_until(
        &app,
        cx,
        |runtime| {
            runtime.node_view(agent_id).and_then(|view| match view {
                runtime::NodeView::Term { snapshot, .. } => snapshot
                    .preview_path_at(0, 2)
                    .as_deref()
                    .filter(|candidate| *candidate == relative_path)
                    .map(|_| ()),
            })
        },
        // available wall time = 200ms；L1 echo 通常 1 个 12ms drain step 即收敛，
        // 额外 188ms = 15 个以上 step，用于吸收测试调度抖动。
        Duration::from_millis(200),
    );
    assert_eq!(
        grid_ready,
        Some(()),
        "Agent 列表中的长路径跨软折行后，首段仍应命中完整 Preview 目标"
    );

    let click = terminal_cell_point(&app, cx, agent_id, 0, 2);
    cx.simulate_click(click, Modifiers::none());
    cx.run_until_parked();

    let (previews, browser_count, agent_exists, selected) = read_runtime(&app, cx, |runtime| {
        let previews = runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| match &node.kind {
                CanvasNodeKind::Preview(spec) => {
                    Some((node.id, std::path::PathBuf::from(&spec.path)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let browser_count = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .count();
        let agent_exists = runtime.document.node(agent_id).is_some();
        let selected = runtime.selection.iter().collect::<Vec<_>>();
        (previews, browser_count, agent_exists, selected)
    });

    assert_eq!(
        previews.len(),
        1,
        "点击软折行路径一次应只创建一张 Preview，不能漏建或重复创建"
    );
    let (preview_id, preview_path) = &previews[0];
    assert_eq!(
        preview_path, &absolute_path,
        "Preview 必须使用跨视觉行恢复后的完整路径，不能只保存首行残片"
    );
    assert_eq!(
        selected,
        vec![*preview_id],
        "软折行路径创建的新 Preview 应成为唯一 selected 节点"
    );
    assert!(agent_exists, "打开软折行路径后原 Agent 卡片必须继续存在");
    assert_eq!(browser_count, 0, "软折行本机路径不得误建 Browser 卡片");
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
    terminal_cell_point(app, cx, node_id, 0, 0)
}

fn terminal_cell_point(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: NodeId,
    row: usize,
    col: usize,
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
        // x = padding(8) + col*cell(8) + half cell(4)
        // y = title(32) + padding(8) + row*line(16) + half line(8)
        let cell_center = Vec2::new(
            top_left.x + (12.0 + col as f32 * 8.0) * zoom,
            top_left.y + (48.0 + row as f32 * 16.0) * zoom,
        );
        point(px(cell_center.x), px(cell_center.y))
    })
}
