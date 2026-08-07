use super::*;
use alacritty_terminal::grid::Scroll;

impl TerminalGrid {
    pub fn scroll_display(&mut self, lines: i32) -> bool {
        if lines == 0 {
            return false;
        }
        let before = self.term.grid().display_offset();
        self.term.scroll_display(Scroll::Delta(lines));
        self.term.grid().display_offset() != before
    }
}

#[cfg(test)]
fn row_text(snapshot: &GridSnapshot, row: usize) -> String {
    snapshot.rows[row].iter().map(|cell| cell.c).collect()
}

#[cfg(test)]
fn row_text_of(rows: &[Vec<Cell>], index: usize) -> String {
    rows[index].iter().map(|cell| cell.c).collect()
}

/// rows_for_absolute 应按 scrollback 绝对行读取完整历史，而不是只看当前视口。
///
/// 数据构造（含关键数值推导）：
///   grid = 4 cols × 3 lines
///   输入 "A0\r\nB1\r\nC2\r\nD3\r\nE4" → history_size = 5 - 3 = 2
///   绝对行 0 = Line(-2) = A0；绝对行 1 = Line(-1) = B1
///   绝对行 2 = Line(0) = C2（屏幕顶行）；绝对行 3 = D3；绝对行 4 = E4
///   当前视口 display_offset = 0，只显示 C2/D3/E4
///
/// 执行过程：
///   1. 构造 4×3 grid 并写入 5 行输出
///   2. rows_for_absolute(0, 4) 读取全部 5 个绝对行
///
/// 预期结果：
///   - 正断言：返回 5 行，逐行内容为 A0/B1/C2/D3/E4
///   - 负断言：不应只有 3 行视口内容（必须跨 scrollback）
#[test]
fn test_rows_for_absolute_reads_full_scrollback() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance(b"A0\r\nB1\r\nC2\r\nD3\r\nE4");

    let rows = grid.rows_for_absolute(0, 4);

    assert_eq!(rows.len(), 5, "绝对行 0..=4 共 5 行，应全部返回");
    assert!(
        row_text_of(&rows, 0).starts_with("A0"),
        "绝对行 0 应是最旧 scrollback 行 A0，实际 {:?}",
        row_text_of(&rows, 0)
    );
    assert!(row_text_of(&rows, 1).starts_with("B1"), "绝对行 1 应是 B1");
    assert!(
        row_text_of(&rows, 2).starts_with("C2"),
        "绝对行 2 应是屏幕顶行 C2"
    );
    assert!(row_text_of(&rows, 3).starts_with("D3"), "绝对行 3 应是 D3");
    assert!(
        row_text_of(&rows, 4).starts_with("E4"),
        "绝对行 4 应是最新行 E4"
    );
    assert_ne!(
        rows.len(),
        3,
        "rows_for_absolute 必须返回完整 scrollback，不能退回只读视口"
    );
}

/// rows_for_absolute 部分范围读取 + 越界钳制。
///
/// 数据构造（含关键数值推导）：
///   grid = 4 cols × 3 lines，输入 5 行 → history_size = 2
///   绝对行范围 [1, 3] = B1/C2/D3
///   越界范围 [0, 99] 应被钳制到 [0, 4]（history_size + screen_lines - 1）
///
/// 执行过程：
///   1. 构造 grid 并写入 5 行
///   2. rows_for_absolute(1, 3) 读取中间三行
///   3. rows_for_absolute(0, 99) 读取越界范围
///
/// 预期结果：
///   - 正断言：[1,3] 返回 B1/C2/D3 三行
///   - 正断言：越界 [0,99] 返回 5 行（钳制到末尾）
///   - 负断言：越界返回不应超过实际可读行数
#[test]
fn test_rows_for_absolute_partial_range_and_clamp() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance(b"A0\r\nB1\r\nC2\r\nD3\r\nE4");

    let middle = grid.rows_for_absolute(1, 3);
    assert_eq!(middle.len(), 3, "[1,3] 应返回 3 行");
    assert!(row_text_of(&middle, 0).starts_with("B1"), "范围首行应为 B1");
    assert!(row_text_of(&middle, 2).starts_with("D3"), "范围末行应为 D3");

    let clamped = grid.rows_for_absolute(0, 99);
    assert_eq!(clamped.len(), 5, "越界范围应钳制到可读行数 5");
    assert!(
        row_text_of(&clamped, 4).starts_with("E4"),
        "越界末行应为最新行 E4"
    );
    assert!(
        clamped.len() <= 5,
        "越界读取不能超出 history_size + screen_lines 总行数"
    );
}

/// scroll_display 正数应回看更旧历史，负数应回到底部可见屏幕。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 4 cols × 3 lines
///   输入 "A0\r\nB1\r\nC2\r\nD3\r\nE4"
///   可见行数 = 3，写入行数 = 5，因此 history_size = 5 - 3 = 2
///   底部 viewport = C2 / D3 / E4
///   scroll_display(+2) 后 display_offset = min(0 + 2, history_size=2) = 2
///   回看 viewport = A0 / B1 / C2
///   scroll_display(-2) 后 display_offset = max(2 - 2, 0) = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. advance 五行输出 → alacritty grid 生成 2 行 scrollback
///   2. snapshot() 读取底部 viewport
///   3. scroll_display(2) → 切到历史顶部
///   4. snapshot() 读取回看 viewport
///   5. scroll_display(-2) → 回到底部
///
/// 预期结果：
///   - 断言 A：底部第一行是 C2，证明多余行已进入 scrollback
///   - 断言 B：回看第一行是 A0，证明 snapshot 按 display_offset 读历史
///   - 断言 C：回到底部后 display_offset 为 0 且第一行恢复 C2
///   - 负断言：回看第一行不应仍是 C2，否则滚轮只改变状态但没有改变渲染内容
#[test]
fn test_scroll_display_reads_history_and_returns_to_bottom() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance(b"A0\r\nB1\r\nC2\r\nD3\r\nE4");

    let bottom = grid.snapshot();
    assert_eq!(
        bottom.history_size, 2,
        "5 行输出减去 3 行 viewport，应形成 2 行 history"
    );
    assert_eq!(
        bottom.display_offset, 0,
        "初始 snapshot 应位于底部 display_offset=0"
    );
    assert!(
        row_text(&bottom, 0).starts_with("C2"),
        "底部 viewport 第一行应是 C2，实际 {:?}",
        row_text(&bottom, 0)
    );

    let changed_up = grid.scroll_display(2);
    let scrolled = grid.snapshot();

    assert!(changed_up, "scroll_display(+2) 应改变 display_offset");
    assert_eq!(
        scrolled.display_offset, 2,
        "回看两行后 display_offset 应为 2"
    );
    assert!(
        row_text(&scrolled, 0).starts_with("A0"),
        "回看 viewport 第一行应是 A0，实际 {:?}",
        row_text(&scrolled, 0)
    );
    assert_ne!(
        row_text(&scrolled, 0),
        row_text(&bottom, 0),
        "回看第一行不应仍等于底部第一行，否则 snapshot 没有应用 display_offset"
    );

    let changed_down = grid.scroll_display(-2);
    let restored = grid.snapshot();

    assert!(changed_down, "scroll_display(-2) 应从历史回到底部");
    assert_eq!(
        restored.display_offset, 0,
        "回到底部后 display_offset 应恢复为 0"
    );
    assert!(
        row_text(&restored, 0).starts_with("C2"),
        "回到底部后第一行应恢复 C2，实际 {:?}",
        row_text(&restored, 0)
    );
}

/// scroll_display 超出顶部/底部时应 clamp，并用返回值表达是否真的移动。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 4 cols × 3 lines
///   输入 5 行 → history_size = 2
///   scroll_display(+99) → min(0 + 99, 2) = 2
///   再次 scroll_display(+1) → min(2 + 1, 2) = 2，不变
///   scroll_display(-99) → max(2 - 99, 0) = 0
///   再次 scroll_display(-1) → max(0 - 1, 0) = 0，不变
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造有 2 行 history 的 grid
///   2. 大步向上滚动并检查 clamp 到顶部
///   3. 在顶部继续向上滚动
///   4. 大步向下滚动并检查 clamp 到底部
///   5. 在底部继续向下滚动
///
/// 预期结果：
///   - 断言 A：第一次向上滚动返回 true 且 display_offset=2
///   - 断言 B：顶部继续向上返回 false
///   - 断言 C：第一次向下滚动返回 true 且 display_offset=0
///   - 负断言：底部继续向下不应返回 true，否则 app 会认为内容发生变化
#[test]
fn test_scroll_display_clamps_at_history_edges() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance(b"A0\r\nB1\r\nC2\r\nD3\r\nE4");

    let moved_to_top = grid.scroll_display(99);
    let top = grid.snapshot();
    assert!(moved_to_top, "从底部向上滚动 99 行应移动到 history 顶部");
    assert_eq!(
        top.display_offset, 2,
        "history_size=2，因此向上滚动 99 行应 clamp 到 display_offset=2"
    );

    let moved_past_top = grid.scroll_display(1);
    let still_top = grid.snapshot();
    assert!(
        !moved_past_top,
        "已在 history 顶部时继续向上滚动不应报告状态变化"
    );
    assert_eq!(
        still_top.display_offset, 2,
        "顶部继续向上滚动后 display_offset 应仍为 2"
    );

    let moved_to_bottom = grid.scroll_display(-99);
    let bottom = grid.snapshot();
    assert!(moved_to_bottom, "从顶部向下滚动 99 行应回到底部");
    assert_eq!(
        bottom.display_offset, 0,
        "向下滚动超过 history 后应 clamp 到底部 display_offset=0"
    );

    let moved_past_bottom = grid.scroll_display(-1);
    assert!(
        !moved_past_bottom,
        "已在底部时继续向下滚动不应返回 true，否则会制造无意义 repaint"
    );
}

/// cursor 落在宽字符 spacer cell 时，snapshot 应回退到宽字符起始列。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 4 cols × 3 lines
///   输入 "你"：中文宽字符占用 col0 + col1(spacer)
///   ANSI "\x1b[1;2H"：移动 cursor 到第 1 行第 2 列，即 0-based col1
///   col1 是 WIDE_CHAR_SPACER，不是真实 glyph 起点
///
/// 执行过程（逐步说明系统如何处理）：
///   1. advance("你") 写入宽字符，alacritty 标记 col1 为 spacer
///   2. advance(CUP 1;2) 将 cursor 移到 spacer cell
///   3. snapshot() 生成中性 GridSnapshot
///
/// 预期结果：
///   - 断言 A：col1 标记为 wide_char_spacer，证明测试前置条件成立
///   - 断言 B：cursor_col 应规范化为 0，指向宽字符真实起始格
///   - 负断言：cursor_col 不应仍为 1，否则 UI 会在 CJK 字符右半格画光标
#[test]
fn test_snapshot_cursor_normalizes_wide_char_spacer_column() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance("你\x1b[1;2H".as_bytes());

    let snapshot = grid.snapshot();

    assert!(
        snapshot.rows[0][1].attrs.wide_char_spacer,
        "第 0 行第 1 列应是宽字符 spacer，实际 attrs={:?}",
        snapshot.rows[0][1].attrs
    );
    assert_eq!(
        snapshot.cursor_col, 0,
        "cursor 位于宽字符 spacer 时应回退到宽字符起始列 0"
    );
    assert_ne!(
        snapshot.cursor_col, 1,
        "cursor_col 不应保留 spacer 列 1，否则 UI 光标会相对 CJK 字符右移一格"
    );
}
