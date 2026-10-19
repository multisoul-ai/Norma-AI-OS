use super::*;
use crate::canvas::terminal_path_cache::PathState;
use crate::cards::term::path_underline::PathMarks;
use gpui::{hsla, px};
use terminal::{Cell, CellAttrs, GridSnapshot, TerminalColor, TerminalNamedColor};

fn cell(c: char) -> Cell {
    Cell {
        c,
        fg: TerminalColor::Named(TerminalNamedColor::Foreground),
        bg: TerminalColor::Named(TerminalNamedColor::Background),
        attrs: CellAttrs::default(),
        hyperlink: None,
    }
}

fn snapshot_with_rows(texts: &[&str], cols: usize) -> GridSnapshot {
    let rows = texts
        .iter()
        .map(|text| {
            text.chars()
                .chain(std::iter::repeat(' '))
                .take(cols)
                .map(cell)
                .collect()
        })
        .collect();
    GridSnapshot {
        rows,
        cursor_line: 0,
        cursor_col: 0,
        cols,
        lines: texts.len(),
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
        show_cursor: true,
    }
}

fn metrics() -> TerminalRenderMetrics {
    TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    }
}

/// 只有项目外（Dashed）的路径才产生虚线区间，项目内（Solid）路径不得出现。
///
/// 数据构造（含关键数值的推导过程）：
///   row0 = "src/main.rs"（11 列，Strong 路径）→ 状态 Exists{outside_project:true} → Dashed
///   row1 = "src/lib.rs"（10 列，Strong 路径）→ 状态 Exists{outside_project:false} → Solid
///   cols = 16，两行共 2 条候选路径
///   cyan = hsla(0.5, 1.0, 0.5, 1.0)，仅用于核对颜色被原样带入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 PathMarks::build 按路径文本分配状态：含 "main" 的为项目外，其余为项目内。
///   2. dash_spans 取 dashed_cells → 只剩 row0 的 (0, 11) 一段。
///
/// 预期结果：
///   - 正断言：恰好 1 个虚线区间，row=0、start_col=0、end_col=11，颜色等于传入的 cyan。
///   - 负断言：row1 的实线路径不得生成虚线区间。
#[test]
fn test_dash_spans_only_cover_outside_project_paths() {
    let snapshot = snapshot_with_rows(&["src/main.rs", "src/lib.rs"], 16);
    let marks = PathMarks::build(&snapshot, |path_ref| PathState::Exists {
        outside_project: path_ref.path.contains("main"),
    });
    let cyan = hsla(0.5, 1.0, 0.5, 1.0);

    let spans = dash_spans(&marks, cyan);

    assert_eq!(spans.len(), 1, "只有项目外路径应生成虚线区间: {spans:?}");
    assert_eq!(
        (spans[0].row, spans[0].start_col, spans[0].end_col),
        (0, 0, 11),
        "虚线区间必须覆盖 row0 的 src/main.rs（列 0..11）"
    );
    assert_eq!(spans[0].color, cyan, "虚线颜色必须原样使用传入的 cyan");
    assert!(
        spans.iter().all(|span| span.row != 1),
        "row1 的项目内实线路径不得生成虚线区间"
    );
}

/// 没有任何路径标记时不生成虚线区间。
///
/// 数据构造：snapshot 仅含 "hello world"（无路径），marks 为 PathMarks::default()（0 行）。
///
/// 预期结果：
///   - 正断言：dash_spans 返回空 Vec。
///   - 负断言：不得因空 marks 而 panic 或产生任何区间。
#[test]
fn test_dash_spans_empty_without_marks() {
    let spans = dash_spans(&PathMarks::default(), hsla(0.0, 0.0, 1.0, 1.0));

    assert!(spans.is_empty(), "无路径标记时不应有虚线区间: {spans:?}");
}

/// 虚线几何：线段、起点与基线位置按单元格尺寸换算。
///
/// 数据构造（含关键数值的推导过程）：
///   cell_width = 8，line_height = 16；span = row1，列 2..5
///   width = 8 × (5 − 2) = 24
///   dash = 8 × 0.5 = 4，gap = 8 × 0.25 = 2，period = 6
///   segments：start 0、6、12、18（均 < 24），长度均为 min(4, 24 − start)
///     start=18 → 24 − 18 = 6 ≥ 4 → 长度 4
///   x0 = 8 × 2 = 16
///   y = 16 × (1 + 1) − 16 × 0.125 = 32 − 2 = 30
///
/// 预期结果：
///   - 正断言：x0 = 16、y = 30、segments = [(0,4),(6,4),(12,4),(18,4)]。
///   - 负断言：不得出现第 5 段（start=24 已触达 width，应停止）。
#[test]
fn test_dash_geometry_converts_cells_to_pixels() {
    let span = TerminalDashSpan {
        row: 1,
        start_col: 2,
        end_col: 5,
        color: hsla(0.5, 1.0, 0.5, 1.0),
    };

    let (x0, segments, y) = dash_geometry(span, metrics());

    assert_eq!(x0, 16.0, "起点 = 8 × 起始列 2");
    assert_eq!(y, 30.0, "y = 16 × 2 − 16 × 0.125");
    assert_eq!(
        segments,
        vec![(0.0, 4.0), (6.0, 4.0), (12.0, 4.0), (18.0, 4.0)],
        "宽 24 按 dash 4 / gap 2 切成 4 段"
    );
    assert_eq!(segments.len(), 4, "不得出现 start=24 的第 5 段");
}
