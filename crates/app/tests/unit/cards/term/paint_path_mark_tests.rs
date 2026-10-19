use super::layout_build::build_layout_with_marks;
use super::*;
use crate::canvas::terminal_path_cache::PathState;
use crate::cards::term::path_underline::PathMarks;
use terminal::CellAttrs;

fn path_snapshot(text: &str) -> GridSnapshot {
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let row: Vec<Cell> = text
        .chars()
        .map(|c| Cell {
            c,
            fg,
            bg,
            attrs: CellAttrs::default(),
            hyperlink: None,
        })
        .collect();
    let cols = row.len();
    GridSnapshot {
        rows: vec![row],
        cursor_line: 0,
        cursor_col: 0,
        cols,
        lines: 1,
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
        show_cursor: false,
    }
}

fn layout_with_state(text: &str, state: PathState) -> (TerminalLayout, TerminalPalette) {
    let palette = TerminalPalette::from_theme(Theme::glass_dark());
    let snapshot = path_snapshot(text);
    let marks = PathMarks::build(&snapshot, |_| state);
    let metrics = TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    };
    let layout = build_layout_with_marks(
        &snapshot,
        palette,
        metrics,
        TerminalGlassStyle::from_theme(Theme::glass_dark()),
        None,
        &marks,
    );
    (layout, palette)
}

/// 项目内已确认存在的路径：青色文字 + 文本自带实线下划线，不产生虚线层。
///
/// 数据构造（含关键数值的推导过程）：
///   row0 = "src/main.rs"，11 个 cell，整段为一个 Strong 路径候选
///   状态 = Exists{outside_project:false} → LinkMark::Solid
///   show_cursor=false，因此不会出现光标层
///
/// 执行过程（逐步说明系统如何处理）：
///   1. PathMarks::build 对该路径给出 Solid。
///   2. build_layout_with_marks 把整段合并为 1 个 text run，样式带 Cyan + underline。
///   3. paint_layer_order 只应包含 TextRuns。
///
/// 预期结果：
///   - 正断言：1 个 run，文本 "src/main.rs"，fg=Cyan，underline=true。
///   - 负断言：path_dashes 为空，paint_layer_order 不含 PathDashes。
#[test]
fn test_build_layout_marks_existing_project_path_with_solid_underline() {
    let (layout, palette) = layout_with_state(
        "src/main.rs",
        PathState::Exists {
            outside_project: false,
        },
    );

    assert_eq!(layout.text_runs.len(), 1, "整条路径应合并为一个 text run");
    let run = &layout.text_runs[0];
    assert_eq!(run.text, "src/main.rs", "run 文本必须是完整路径");
    assert_eq!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Cyan)),
        "可点击路径应使用青色链接色"
    );
    assert!(run.style.underline, "项目内路径应带实线下划线");
    assert!(layout.path_dashes.is_empty(), "项目内路径不得生成虚线区间");
    assert!(
        !paint_layer_order(&layout).contains(&TerminalPaintLayer::PathDashes),
        "没有虚线时不应出现 PathDashes 绘制层"
    );
}

/// 项目外已确认存在的路径：青色文字、文本无下划线，另由 PathDashes 层画虚线。
///
/// 数据构造（含关键数值的推导过程）：
///   row0 = "src/main.rs"，11 个 cell；状态 = Exists{outside_project:true} → LinkMark::Dashed
///   预期虚线区间 = row0 的 cols 0..11，共 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dashed 标记使文字变青但 underline=false（避免与虚线重叠成实线）。
///   2. layout.path_dashes 记录 (row0, 0..11)。
///   3. paint_layer_order：TextRuns 之后紧跟 PathDashes。
///
/// 预期结果：
///   - 正断言：fg=Cyan、underline=false；path_dashes 恰好 1 条且区间正确。
///   - 正断言：图层顺序为 [TextRuns, PathDashes]。
///   - 负断言：不得带实线 underline。
#[test]
fn test_build_layout_marks_outside_project_path_with_dashes_not_solid() {
    let (layout, palette) = layout_with_state(
        "src/main.rs",
        PathState::Exists {
            outside_project: true,
        },
    );

    let run = &layout.text_runs[0];
    assert_eq!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Cyan)),
        "项目外路径同样使用青色提示可点击"
    );
    assert!(!run.style.underline, "项目外路径不得使用实线下划线");
    assert_eq!(layout.path_dashes.len(), 1, "应恰好有一条虚线区间");
    assert_eq!(
        (
            layout.path_dashes[0].row,
            layout.path_dashes[0].start_col,
            layout.path_dashes[0].end_col
        ),
        (0, 0, 11),
        "虚线区间必须覆盖整条路径 cols 0..11"
    );
    assert_eq!(
        paint_layer_order(&layout),
        vec![TerminalPaintLayer::TextRuns, TerminalPaintLayer::PathDashes],
        "虚线必须画在文字之后"
    );
}

/// 已确认不存在的路径：保持普通前景色，没有任何下划线。
///
/// 数据构造：row0 = "src/main.rs"（Strong），状态 = Missing → underline_decision 返回 None。
///
/// 预期结果：
///   - 正断言：fg=Foreground，underline=false。
///   - 负断言：path_dashes 为空，fg 不得为 Cyan。
#[test]
fn test_build_layout_leaves_missing_path_unmarked() {
    let (layout, palette) = layout_with_state("src/main.rs", PathState::Missing);

    let run = &layout.text_runs[0];
    assert_eq!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Foreground)),
        "不存在的路径应保持默认前景色"
    );
    assert!(!run.style.underline, "不存在的路径不得带下划线");
    assert!(layout.path_dashes.is_empty(), "不存在的路径不得有虚线");
    assert_ne!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Cyan)),
        "不存在的路径不得使用链接色"
    );
}
