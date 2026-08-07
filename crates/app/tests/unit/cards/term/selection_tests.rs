use super::*;
use crate::cards::term::paint::TerminalRenderMetrics;
use gpui::{px, Point};
use terminal::{Cell, CellAttrs, GridSnapshot, TerminalColor, TerminalNamedColor};

fn default_cell(c: char) -> Cell {
    Cell {
        c,
        fg: TerminalColor::Named(TerminalNamedColor::Foreground),
        bg: TerminalColor::Named(TerminalNamedColor::Background),
        attrs: CellAttrs::default(),
        hyperlink: None,
    }
}

fn row_from_str(line: &str, cols: usize) -> Vec<Cell> {
    let mut row = line.chars().map(default_cell).collect::<Vec<_>>();
    row.resize(cols, default_cell(' '));
    row
}

fn snapshot_from_lines(lines: &[&str], cols: usize) -> GridSnapshot {
    let rows = lines
        .iter()
        .map(|line| row_from_str(line, cols))
        .collect::<Vec<_>>();
    GridSnapshot {
        lines: rows.len(),
        rows,
        cols,
        cursor_line: 0,
        cursor_col: 0,
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
    }
}

/// terminal point 映射：正文局部坐标应按 cell 尺寸 clamp 到合法 grid 坐标。
///
/// 数据构造（含关键数值推导）：
///   metrics.cell_width = 8px，line_height = 16px
///   point = (19px, 33px)
///   col = floor(19 / 8) = 2；row = floor(33 / 16) = 2
///   snapshot cols=5, lines=3，因此 row=2/col=2 均合法
///
/// 执行过程：
///   1. 构造 5x3 snapshot 与 metrics
///   2. terminal_point_for_local_position(point=(19,33))
///   3. 检查返回 TextPoint
///
/// 预期结果：
///   - 正断言：返回 row=2,col=2
///   - 负断言：不应四舍五入成 col=3 或 row clamp 到 1
#[test]
fn test_terminal_point_for_local_position_floors_and_clamps_to_grid() {
    let snapshot = snapshot_from_lines(&["abcde", "fghij", "klmno"], 5);
    let metrics = TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    };

    let point =
        terminal_point_for_local_position(Point::new(px(19.0), px(33.0)), &snapshot, metrics);

    assert_eq!(
        point,
        crate::canvas::text_selection::TextPoint::new(2, 2),
        "19/8 与 33/16 都应向下取整，得到 row=2,col=2"
    );
    assert_ne!(
        point,
        crate::canvas::text_selection::TextPoint::new(2, 3),
        "列坐标不应四舍五入到 3，否则鼠标在 cell 左半边会错位"
    );
}

/// terminal 单行复制：只返回选中列，并裁掉右侧填充空格。
///
/// 数据构造（含关键数值推导）：
///   row 0 = "abcdef" + 4 个 terminal padding 空格，cols=10
///   range = row 0 col 1 → row 0 col 4
///   选中字符为 b,c,d，因此结果应为 "bcd"
///
/// 执行过程：
///   1. 构造一行 snapshot
///   2. terminal_selected_text(range=1..4)
///   3. 检查复制文本
///
/// 预期结果：
///   - 正断言：复制结果是 "bcd"
///   - 负断言：不应复制整行 "abcdef" 或包含右侧 padding
#[test]
fn test_terminal_selected_text_copies_single_line_columns_only() {
    let snapshot = snapshot_from_lines(&["abcdef"], 10);
    let range = crate::canvas::text_selection::TextRange::new(
        crate::canvas::text_selection::TextPoint::new(0, 1),
        crate::canvas::text_selection::TextPoint::new(0, 4),
    );

    let text = terminal_selected_text(&snapshot, range);

    assert_eq!(text, "bcd", "单行选区 col 1..4 应只复制 bcd");
    assert_ne!(text, "abcdef", "单行选区不应退化为整行复制");
    assert!(
        !text.ends_with(' '),
        "复制文本不应包含 terminal grid 右侧填充空格"
    );
}

/// terminal 跨行复制：跨行选择应保留换行，并裁掉每行右侧填充。
///
/// 数据构造（含关键数值推导）：
///   row 0 = "abcde"，row 1 = "xy"，row 2 = "12345"，cols=8
///   range = row 0 col 2 → row 2 col 3
///   row0 复制 cde；row1 复制 xy；row2 复制 123
///
/// 执行过程：
///   1. 构造三行 snapshot
///   2. terminal_selected_text(range=(0,2)..(2,3))
///   3. 检查换行和裁剪结果
///
/// 预期结果：
///   - 正断言：结果为 "cde\nxy\n123"
///   - 负断言：中间行不应丢失，结果不应带 padding 空格
#[test]
fn test_terminal_selected_text_copies_multiline_range_with_newlines() {
    let snapshot = snapshot_from_lines(&["abcde", "xy", "12345"], 8);
    let range = crate::canvas::text_selection::TextRange::new(
        crate::canvas::text_selection::TextPoint::new(0, 2),
        crate::canvas::text_selection::TextPoint::new(2, 3),
    );

    let text = terminal_selected_text(&snapshot, range);

    assert_eq!(text, "cde\nxy\n123", "跨行选区应按每行片段复制并用换行连接");
    assert!(
        text.contains('\n'),
        "跨行复制必须保留换行，否则多行命令输出会被粘成一行"
    );
    assert!(
        !text.contains("   "),
        "复制结果不应包含 terminal grid 右侧 padding 空格"
    );
}

/// terminal 宽字符 spacer：wide-char spacer 不应重复复制前一个宽字符。
///
/// 数据构造（含关键数值推导）：
///   row = ['界', spacer, 'A', ' ']，cols=4
///   spacer 位于 col 1，表示 col 0 的宽字符占位
///   range = row 0 col 0 → row 0 col 3
///
/// 执行过程：
///   1. 构造带 wide_char_spacer 的 row
///   2. terminal_selected_text(range=0..3)
///   3. 检查复制文本
///
/// 预期结果：
///   - 正断言：结果包含一次 "界" 和 "A"
///   - 负断言：结果不应重复为 "界界A"
#[test]
fn test_terminal_selected_text_does_not_duplicate_wide_char_spacer() {
    let mut spacer = default_cell('界');
    spacer.attrs.wide_char_spacer = true;
    let snapshot = GridSnapshot {
        rows: vec![vec![
            default_cell('界'),
            spacer,
            default_cell('A'),
            default_cell(' '),
        ]],
        cursor_line: 0,
        cursor_col: 0,
        cols: 4,
        lines: 1,
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
    };
    let range = crate::canvas::text_selection::TextRange::new(
        crate::canvas::text_selection::TextPoint::new(0, 0),
        crate::canvas::text_selection::TextPoint::new(0, 3),
    );

    let text = terminal_selected_text(&snapshot, range);

    assert_eq!(
        text, "界 A",
        "wide-char spacer 应作为占位空格处理，避免重复复制宽字符"
    );
    assert_ne!(
        text, "界界A",
        "spacer 不应被当作第二个宽字符复制，否则中文/emoji 会重复"
    );
}

/// terminal_text_from_absolute_rows：从完整 scrollback 行数据构造选中文本，
/// 即使选中范围横跨 scrollback 与可见行也应完整返回。
///
/// 数据构造（含关键数值推导）：
///   rows 共 5 行，代表绝对行 0..=4：
///     abs 0 = "aa"、abs 1 = "bb"、abs 2 = "cc"、abs 3 = "dd"、abs 4 = "ee"
///   cols = 4（每行右侧有 padding 空格）
///   base_abs = 0（rows[0] 对应绝对行 0）
///   range = 绝对行 1 col 1 → 绝对行 3 col 2
///   预期：行1 取 [1,4]→"b " 裁尾"b"；行2 取 [0,4]→"cc"；行3 取 [0,2]→"dd"→"dd"[0,2]="dd"
///
/// 执行过程：
///   1. 构造 5 行 scrollback 行数据
///   2. terminal_text_from_absolute_rows(range=(1,1)..(3,2))
///   3. 检查拼接结果
///
/// 预期结果：
///   - 正断言：结果为 "b\ncc\ndd"
///   - 负断言：不应只返回可见部分或缺失中间行
#[test]
fn test_terminal_text_from_absolute_rows_spans_scrollback() {
    let rows = vec![
        row_from_str("aa", 4),
        row_from_str("bb", 4),
        row_from_str("cc", 4),
        row_from_str("dd", 4),
        row_from_str("ee", 4),
    ];
    let range = crate::canvas::text_selection::TextRange::new(
        crate::canvas::text_selection::TextPoint::new(1, 1),
        crate::canvas::text_selection::TextPoint::new(3, 2),
    );

    let text = terminal_text_from_absolute_rows(&rows, 4, 0, range);

    assert_eq!(text, "b\ncc\ndd", "绝对行范围 1..3 应逐行复制并用换行连接");
    assert_ne!(
        text, "b\ncc",
        "末行 abs3 不应被丢弃，否则跨 scrollback 选中会截断尾部"
    );
    assert!(
        !text.contains("aa") && !text.contains("ee"),
        "范围之外的行不应混入结果"
    );
}

/// terminal_selection_spans：绝对行坐标应裁剪到当前视口并转成视口相对行。
///
/// 数据构造（含关键数值推导）：
///   snapshot: rows 2 行、history_size=3、display_offset=1
///   视口顶绝对行 = history_size - display_offset = 2；视口覆盖绝对行 2..3
///   range = 绝对行 1 col 0 → 绝对行 3 col 2
///   spans_for_columns 产出行 1(0..cols)、行2(0..cols)、行3(0..2)
///   裁剪后：行1 不在视口被丢弃；行2 → 视口行0；行3 → 视口行1
///
/// 执行过程：
///   1. 构造带 scrollback 的 snapshot
///   2. terminal_selection_spans(range=(1,0)..(3,2))
///
/// 预期结果：
///   - 正断言：返回 2 个 span，视口行 0 和 1
///   - 负断言：绝对行 1（视口上方）不应出现
#[test]
fn test_terminal_selection_spans_clip_to_viewport() {
    let snapshot = GridSnapshot {
        rows: vec![row_from_str("xx", 3), row_from_str("yy", 3)],
        cursor_line: 0,
        cursor_col: 0,
        cols: 3,
        lines: 2,
        display_offset: 1,
        history_size: 3,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
    };
    let range = crate::canvas::text_selection::TextRange::new(
        crate::canvas::text_selection::TextPoint::new(1, 0),
        crate::canvas::text_selection::TextPoint::new(3, 2),
    );

    let spans = terminal_selection_spans(&snapshot, range);

    let rows: Vec<usize> = spans.iter().map(|s| s.row).collect();
    assert_eq!(rows, vec![0, 1], "应只剩视口内的 2 行，转为视口相对行 0/1");
}
