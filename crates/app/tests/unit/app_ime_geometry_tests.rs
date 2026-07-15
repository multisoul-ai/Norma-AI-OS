use super::*;
use canvas::CanvasRect;
use gpui::px;
use runtime::{InputMode, NodeScroll};

/// 聚焦 terminal 的 IME bounds 应返回当前光标矩形，供系统候选窗贴近光标。
///
/// 数据构造（含关键数值的推导过程）：
///   node bounds = origin(100, 50), size(640x360)
///   viewport 默认 zoom = 1, pan = (0,0)，因此 node screen top-left = (100,50)
///   title height = TITLE_BAR_H(44) * zoom(1) = 44
///   terminal padding = TERM_PADDING_PX(8) * zoom(1) = 8
///   初始 terminal grid cursor = line 0, col 0
///   expected x = node_left(100) + padding(8) + col(0)*cell(8) = 108
///   expected y = node_top(50) + title(44) + padding(8) + line(0)*line_height(16) = 102
///   expected size = cell_width(8) x line_height(16)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime 新建 /bin/sh terminal，获得 node_id
///   2. focus_node(node_id)，模拟用户正在该 terminal 输入中文
///   3. terminal_ime_cursor_bounds(Canvas) 根据 focused node + GridSnapshot cursor 计算 bounds
///
/// 预期结果：
///   - 正断言：返回 Some(bounds)，macOS IME 才能定位候选窗
///   - 正断言：bounds origin 精确落在 terminal 内容区光标位置
///   - 正断言：bounds size 等于一个终端 cell 的宽高
///   - 负断言：focus 不应停留在 CanvasSelect，否则测试前置状态无效
#[test]
fn test_terminal_ime_cursor_bounds_uses_focused_terminal_cursor_cell() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(
            CanvasRect::new(100.0, 50.0, 640.0, 360.0),
            "/bin/sh",
            80,
            24,
        )
        .expect("test should spawn /bin/sh terminal");
    runtime.focus.focus_node(node_id);

    let bounds = terminal_ime_cursor_bounds(&runtime, AppScreen::Canvas)
        .expect("focused terminal should expose cursor bounds for IME");

    assert_eq!(
        runtime.focus.mode(),
        InputMode::NodeFocused(node_id),
        "测试前置条件应聚焦新建 terminal"
    );
    assert_ne!(
        runtime.focus.mode(),
        InputMode::CanvasSelect,
        "测试前置条件不应停留在 CanvasSelect，否则 IME bounds 没有目标节点"
    );
    assert_eq!(
        bounds.origin.x,
        px(108.0),
        "IME bounds x 应等于 node_left + terminal padding + cursor_col * cell_width"
    );
    assert_eq!(
        bounds.origin.y,
        px(102.0),
        "IME bounds y 应等于 node_top + title height + terminal padding + cursor_line * line_height"
    );
    assert_eq!(
        bounds.size.width,
        px(8.0),
        "IME bounds width 应等于 terminal cell width"
    );
    assert_eq!(
        bounds.size.height,
        px(16.0),
        "IME bounds height 应等于 terminal line height"
    );

    runtime.kill_all();
}

/// terminal 正在回看 scrollback 时 IME bounds 应隐藏，避免候选窗贴到历史 viewport 的错位光标。
///
/// 数据构造（含关键数值的推导过程）：
///   node bounds = origin(100, 50), size(320x140)
///   terminal size = 4 列 × 3 行
///   输出 5 行 A0/B1/C2/D3/E4 → history_size = 5 - 3 = 2
///   scroll_node(TermLines(2)) → display_offset 从 0 变为 2，当前 viewport 显示历史顶部
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime 新建 /bin/sh terminal，并写入 5 行输出
///   2. drain_all 直到输出进入 terminal grid
///   3. focus_node(node_id)，模拟用户仍聚焦该 terminal
///   4. scroll_node(node_id, TermLines(2)) 进入 scrollback 历史
///   5. terminal_ime_cursor_bounds(Canvas) 查询 IME 候选窗位置
///
/// 预期结果：
///   - 正断言：display_offset=2，确认测试进入历史回看状态
///   - 正断言：IME bounds 返回 None，系统候选窗不应跟随不可见 live cursor
///   - 负断言：IME bounds 不应返回 Some，否则会把候选窗定位到历史 viewport 上的错位 cursor
#[test]
fn test_terminal_ime_cursor_bounds_hidden_during_scrollback() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(CanvasRect::new(100.0, 50.0, 320.0, 140.0), "/bin/sh", 4, 3)
        .expect("test should spawn /bin/sh terminal");
    runtime
        .send_node_input(
            node_id,
            runtime::NodeInput::Text("printf 'A0\\r\\nB1\\r\\nC2\\r\\nD3\\r\\nE4'; exit\n".into()),
        )
        .expect("test command should be written to terminal");
    for _ in 0..50 {
        runtime.drain_all();
        if let Some(runtime::NodeView::Term { status, .. }) = runtime.node_view(node_id) {
            if !matches!(status, terminal::TerminalStatus::Running) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    runtime.focus.focus_node(node_id);
    runtime
        .scroll_node(node_id, NodeScroll::TermLines(2))
        .expect("terminal should support scrollback");
    let snapshot = match runtime
        .node_view(node_id)
        .expect("terminal should expose a NodeView after scroll")
    {
        runtime::NodeView::Term { snapshot, .. } => snapshot,
    };

    let bounds = terminal_ime_cursor_bounds(&runtime, AppScreen::Canvas);

    assert_eq!(
        snapshot.display_offset, 2,
        "测试前置条件应已进入 scrollback，display_offset 应为 2"
    );
    assert!(
        bounds.is_none(),
        "scrollback 回看历史时 IME bounds 应为 None，实际为 {bounds:?}"
    );
    runtime.kill_all();
}
