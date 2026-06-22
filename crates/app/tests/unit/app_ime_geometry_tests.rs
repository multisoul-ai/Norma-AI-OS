use super::*;
use canvas::CanvasRect;
use gpui::px;
use runtime::InputMode;

/// 聚焦 terminal 的 IME bounds 应返回当前光标矩形，供系统候选窗贴近光标。
///
/// 数据构造（含关键数值的推导过程）：
///   node bounds = origin(100, 50), size(640x360)
///   viewport 默认 zoom = 1, pan = (0,0)，因此 node screen top-left = (100,50)
///   title height = TITLE_BAR_H(26) * zoom(1) = 26
///   terminal padding = TERM_PADDING_PX(8) * zoom(1) = 8
///   初始 terminal grid cursor = line 0, col 0
///   expected x = node_left(100) + padding(8) + col(0)*cell(8) = 108
///   expected y = node_top(50) + title(26) + padding(8) + line(0)*line_height(16) = 84
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
        px(84.0),
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
