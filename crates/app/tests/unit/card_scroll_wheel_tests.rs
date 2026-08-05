use super::*;
use crate::canvas::text_selection::TextPoint;
use runtime::NodeScroll;

/// 选中拖拽越界自动滚动方向：指针在内容区上方回看历史、下方回到底部、区内不滚动。
///
/// 数据构造（含关键数值推导）：
///   content_height = 240.0（终端内容区高度，示意值）
///   local_y = -10.0  → 指针在内容区上方 → 应回看历史（+1）
///   local_y = 120.0  → 指针在内容区内 → 不应滚动（0）
///   local_y = 260.0  → 指针在内容区下方 → 应回到底部（-1）
///
/// 执行过程：
///   1. 分别用三个 local_y 调用 terminal_selection_auto_scroll_lines
///   2. 检查返回的行方向
///
/// 预期结果：
///   - 正断言：上方 +1、下方 -1、区内 0
///   - 负断言：区内不应产生 ±1 滚动，否则 mouse_move 会误滚动
#[test]
fn test_terminal_selection_auto_scroll_lines_direction() {
    assert_eq!(
        terminal_selection_auto_scroll_lines(-10.0, 240.0),
        1,
        "指针在内容区上方时应回看历史（+1 行）"
    );
    assert_eq!(
        terminal_selection_auto_scroll_lines(260.0, 240.0),
        -1,
        "指针在内容区下方时应回到底部（-1 行）"
    );
    assert_eq!(
        terminal_selection_auto_scroll_lines(120.0, 240.0),
        0,
        "指针在内容区内时不应触发自动滚动"
    );
    assert_ne!(
        terminal_selection_auto_scroll_lines(120.0, 240.0),
        1,
        "内容区内指针不应误判为向上滚动"
    );
}

/// app 层将 terminal-owned wheel 转成 NodeScroll：有正文坐标时用 TermWheel，缺失坐标时回退 TermLines。
///
/// 数据构造（含关键数值推导）：
///   lines = +2，来自 +32px / 16px = 2 行
///   text point = row 3, col 7，均在 u16 范围内，应保留为 TermWheel 坐标
///   fallback text point = None，说明 session gone 或无法读取 snapshot，应使用旧 TermLines(2)
///
/// 执行过程：
///   1. terminal_scroll_for_text_point(2, Some(row=3,col=7))
///   2. terminal_scroll_for_text_point(2, None)
///   3. 分别检查 NodeScroll variant
///
/// 预期结果：
///   - 正断言：有坐标时返回 TermWheel{lines=2,row=3,col=7}
///   - 正断言：无坐标时返回 TermLines(2)，兼容旧 scrollback
///   - 负断言：有坐标时不应退回 TermLines，否则 OpenCode TUI 收不到 row/col wheel
#[test]
fn test_terminal_scroll_for_text_point_prefers_term_wheel_with_fallback() {
    let with_point = terminal_scroll_for_text_point(2, Some(TextPoint::new(3, 7)));
    let fallback = terminal_scroll_for_text_point(2, None);

    assert_eq!(
        with_point,
        NodeScroll::TermWheel {
            lines: 2,
            row: 3,
            col: 7
        },
        "terminal 正文命中坐标存在时应发送 TermWheel，供 runtime 根据 TUI mode 分流"
    );
    assert_eq!(
        fallback,
        NodeScroll::TermLines(2),
        "无法计算 terminal 坐标时应回退旧 TermLines scrollback"
    );
    assert_ne!(
        with_point,
        NodeScroll::TermLines(2),
        "有 row/col 时不应退回 TermLines，否则 OpenCode TUI wheel 会继续失效"
    );
}
