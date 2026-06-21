use super::*;
use terminal::CellAttrs;

fn test_palette() -> TerminalPalette {
    TerminalPalette::from_theme(Theme::glass_dark())
}

fn test_cell(c: char, fg: TerminalColor, bg: TerminalColor, attrs: CellAttrs) -> Cell {
    Cell { c, fg, bg, attrs }
}

/// 相邻同样式字符合并为一个 text run，样式变化切分 run。
///
/// 数据构造：
///   row[0] = 'A'，fg=Foreground，bg=Background，attrs=default
///   row[1] = 'B'，fg=Foreground，bg=Background，attrs=default
///   row[2] = 'C'，fg=Red，bg=Background，attrs=default
///
/// 执行过程：
///   1. layout_row(row=0) 解析三格样式
///   2. A/B 的 fg/bg/attrs 完全一致 → 合并为 "AB"
///   3. C 的 fg 从 Foreground 变为 Red → 切分为第二个 run
///
/// 预期结果：
///   - 正断言：生成 2 个 text run
///   - 正断言：第一个 run 文本为 "AB"，start_col=0
///   - 正断言：第二个 run 文本为 "C"，start_col=2
///   - 负断言：不应生成 1 个 run，否则颜色变化会被吞掉
#[test]
fn test_layout_row_merges_same_style_and_splits_color_change() {
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let row = vec![
        test_cell('A', fg, bg, CellAttrs::default()),
        test_cell('B', fg, bg, CellAttrs::default()),
        test_cell(
            'C',
            TerminalColor::Named(TerminalNamedColor::Red),
            bg,
            CellAttrs::default(),
        ),
    ];

    let layout = layout_row(0, &row, test_palette());

    assert_eq!(
        layout.text_runs.len(),
        2,
        "A/B 应合并且 C 应因颜色变化切分，所以应有 2 个 run"
    );
    assert_eq!(
        layout.text_runs[0].text, "AB",
        "同样式相邻字符 A/B 应合并为一个 run"
    );
    assert_eq!(
        layout.text_runs[0].start_col, 0,
        "第一个 run 应从第 0 列开始"
    );
    assert_eq!(
        layout.text_runs[1].text, "C",
        "颜色变化后的 C 应成为独立 run"
    );
    assert_eq!(
        layout.text_runs[1].start_col, 2,
        "第二个 run 应从第 2 列开始"
    );
    assert_ne!(
        layout.text_runs.len(),
        1,
        "颜色变化不应被合并进单一 run，否则样式会丢失"
    );
}

/// inverse 在 app 层解析时交换前景色与背景色。
///
/// 数据构造：
///   cell.c = 'I'
///   fg = Red，bg = Blue
///   attrs.inverse = true
///
/// 执行过程：
///   1. palette.resolve(Red) 得到 red
///   2. palette.resolve(Blue) 得到 blue
///   3. resolve_cell_style 看到 inverse=true → fg/bg 交换
///
/// 预期结果：
///   - 正断言：解析后 fg == 原始 Blue
///   - 正断言：解析后 bg == 原始 Red
///   - 负断言：解析后 fg 不应仍等于原始 Red
#[test]
fn test_inverse_swaps_foreground_and_background() {
    let palette = test_palette();
    let red = TerminalColor::Named(TerminalNamedColor::Red);
    let blue = TerminalColor::Named(TerminalNamedColor::Blue);
    let mut attrs = CellAttrs::default();
    attrs.inverse = true;
    let cell = test_cell('I', red, blue, attrs);

    let style = resolve_cell_style(&cell, palette);

    assert_eq!(
        style.fg,
        palette.resolve(blue),
        "inverse 后前景色应变为原始背景色 Blue"
    );
    assert_eq!(
        style.bg,
        palette.resolve(red),
        "inverse 后背景色应变为原始前景色 Red"
    );
    assert_ne!(
        style.fg,
        palette.resolve(red),
        "inverse 后前景色不应仍是原始 Red"
    );
}

/// hidden 字符保留背景但不生成可见文字 run。
///
/// 数据构造：
///   row[0] = 'H'
///   fg = Foreground
///   bg = Red（非默认背景，便于观察 background rect）
///   attrs.hidden = true
///
/// 执行过程：
///   1. layout_row(row=0) 解析 hidden cell
///   2. hidden 不生成 text run
///   3. 背景仍进入 background rect，供 UI 绘制
///
/// 预期结果：
///   - 正断言：生成 1 个背景 rect，颜色为 Red
///   - 正断言：背景 rect 覆盖第 0 列
///   - 负断言：不生成 text run，否则 hidden 字符会被显示出来
#[test]
fn test_hidden_cell_paints_background_without_text_run() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Red);
    let mut attrs = CellAttrs::default();
    attrs.hidden = true;
    let row = vec![test_cell('H', fg, bg, attrs)];

    let layout = layout_row(0, &row, palette);

    assert_eq!(
        layout.backgrounds.len(),
        1,
        "hidden cell 仍应生成背景 rect 以保留背景样式"
    );
    assert_eq!(
        layout.backgrounds[0].color,
        palette.resolve(bg),
        "hidden cell 的背景颜色应保留为 Red"
    );
    assert_eq!(
        layout.backgrounds[0].start_col, 0,
        "hidden cell 背景 rect 应从第 0 列开始"
    );
    assert_eq!(
        layout.backgrounds[0].end_col, 1,
        "hidden cell 背景 rect 应覆盖第 0 列后在第 1 列结束"
    );
    assert!(
        layout.text_runs.is_empty(),
        "hidden cell 不应生成可见 text run"
    );
}

/// canvas 文本原点应在行框内保留垂直余量，避免首行贴边后被裁剪。
///
/// 数据构造（含关键数值推导）：
///   bounds.origin = (10, 20)
///   run.row = 1，start_col = 2
///   metrics.cell_width = 8，line_height = 16，font_size = 13
///   x = 10 + 8 * 2 = 26
///   vertical_offset = (16 - 13) / 2 = 1.5
///   y = 20 + 16 * 1 + 1.5 = 37.5
///
/// 执行过程：
///   1. text_run_origin 根据 cell 网格定位 run 起点
///   2. vertical_text_offset 把 glyph 下移到行框内部
///
/// 预期结果：
///   - 正断言：x 精确落在第 2 列，即 26
///   - 正断言：y 包含 1.5px 垂直余量，即 37.5
///   - 负断言：y 不应停在无余量的 36，否则首行容易被 canvas 上边界裁剪
#[test]
fn test_text_run_origin_keeps_glyph_inside_line_box() {
    let palette = test_palette();
    let bounds = Bounds {
        origin: point(px(10.0), px(20.0)),
        size: Size {
            width: px(200.0),
            height: px(100.0),
        },
    };
    let run = TerminalTextRun {
        text: "AB".to_string(),
        row: 1,
        start_col: 2,
        end_col: 4,
        style: TerminalTextStyle {
            fg: palette.foreground,
            bg: palette.background,
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
        },
    };
    let metrics = TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    };

    let origin = text_run_origin(bounds, &run, metrics);

    assert_eq!(
        origin.x,
        px(26.0),
        "文本 run 的 x 应为 bounds.x + cell_width * start_col = 10 + 8 * 2"
    );
    assert_eq!(
        origin.y,
        px(37.5),
        "文本 run 的 y 应包含 (line_height - font_size) / 2 = 1.5px 的垂直余量"
    );
    assert_ne!(
        origin.y,
        px(36.0),
        "文本 run 不应贴在行框顶部，否则首行可能被裁剪到只剩半截"
    );
}
