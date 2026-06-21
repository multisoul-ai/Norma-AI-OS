use super::*;
use terminal::CellAttrs;

fn test_palette() -> TerminalPalette {
    TerminalPalette::from_theme(Theme::glass_dark())
}

fn test_cell(c: char, fg: TerminalColor, bg: TerminalColor, attrs: CellAttrs) -> Cell {
    Cell { c, fg, bg, attrs }
}

fn test_metrics() -> TerminalRenderMetrics {
    TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    }
}

fn test_glass_style() -> TerminalGlassStyle {
    TerminalGlassStyle::from_theme(Theme::glass_dark())
}

fn test_snapshot(row: Vec<Cell>) -> GridSnapshot {
    let cols = row.len();
    GridSnapshot {
        rows: vec![row],
        cursor_line: 0,
        cursor_col: 0,
        cols,
        lines: 1,
    }
}

fn test_snapshot_with_cursor(
    rows: Vec<Vec<Cell>>,
    cursor_line: usize,
    cursor_col: usize,
) -> GridSnapshot {
    let cols = rows.first().map(|row| row.len()).unwrap_or(0);
    let lines = rows.len();
    GridSnapshot {
        rows,
        cursor_line,
        cursor_col,
        cols,
        lines,
    }
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

/// 默认终端背景应由外层 Liquid Glass 内容容器承载，canvas 不再重复整块填充。
///
/// 数据构造（含关键数值推导）：
///   row[0] = 'A'
///   fg = Foreground，bg = Background，attrs=default
///   theme.glass_dark().term_bg alpha = 0xfa / 0xff ≈ 98%
///   旧行为：容器 98% + canvas 98% → 1 - (1-0.98)^2 ≈ 99.96%，玻璃近似实心
///
/// 执行过程：
///   1. build_layout 解析默认背景 cell
///   2. layout.default_bg 保留颜色语义，供过滤默认背景与 ANSI inverse 使用
///   3. layout.default_bg_fill 不请求 canvas 整块背景，避免重复叠加 term_bg
///
/// 预期结果：
///   - 正断言：default_bg 仍等于主题默认终端背景
///   - 正断言：default_bg_fill == None，默认底色交给外层容器
///   - 负断言：default_bg_fill 不应是 Some(default_bg)，否则 Liquid Glass 会被重复盖实
#[test]
fn test_layout_does_not_fill_canvas_with_default_background() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let snapshot = test_snapshot(vec![test_cell('A', fg, bg, CellAttrs::default())]);

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());

    assert_eq!(
        layout.default_bg,
        palette.resolve(bg),
        "layout 仍应保留默认背景色语义，供默认背景过滤和 inverse 解析使用"
    );
    assert_eq!(
        layout.default_bg_fill, None,
        "canvas 不应再整块填充默认背景，默认底色应由 Liquid Glass 容器承担"
    );
    assert_ne!(
        layout.default_bg_fill,
        Some(layout.default_bg),
        "canvas 若再次填充 default_bg，会和外层 term_bg 叠成近似不透明"
    );
}

/// 显式 ANSI 背景色仍应绘制，不能因为去掉默认整块填充而丢失样式。
///
/// 数据构造（含关键数值推导）：
///   row[0] = 'D'，bg = Background（默认背景）
///   row[1] = 'R'，bg = Red（显式 ANSI 背景）
///   默认背景应被过滤，Red 背景应保留
///   待绘制背景数量 = 1（只剩 row[1] 的 Red）
///
/// 执行过程：
///   1. layout_row 合并并记录默认背景 rect 与 Red 背景 rect
///   2. build_layout 保存 default_bg
///   3. paintable_backgrounds 过滤 default_bg，只返回显式背景
///
/// 预期结果：
///   - 正断言：待绘制背景只有 1 个
///   - 正断言：该背景颜色是 Red，覆盖第 1 列
///   - 负断言：待绘制背景不应包含 default_bg，否则默认底色会重复叠加
#[test]
fn test_paintable_backgrounds_keep_explicit_ansi_background_only() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let default_bg = TerminalColor::Named(TerminalNamedColor::Background);
    let red_bg = TerminalColor::Named(TerminalNamedColor::Red);
    let snapshot = test_snapshot(vec![
        test_cell('D', fg, default_bg, CellAttrs::default()),
        test_cell('R', fg, red_bg, CellAttrs::default()),
    ]);

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let paintable: Vec<_> = paintable_backgrounds(&layout).collect();

    assert_eq!(
        paintable.len(),
        1,
        "默认背景应被过滤，显式 Red 背景应保留，因此只应绘制 1 个背景 rect"
    );
    assert_eq!(
        paintable[0].color,
        palette.resolve(red_bg),
        "保留下来的背景 rect 应是显式 ANSI Red"
    );
    assert_eq!(paintable[0].start_col, 1, "显式 Red 背景应从第 1 列开始");
    assert_eq!(
        paintable[0].end_col, 2,
        "显式 Red 背景应覆盖第 1 列后在第 2 列结束"
    );
    assert!(
        paintable.iter().all(|bg| bg.color != layout.default_bg),
        "待绘制背景中不应包含 default_bg，否则会重复盖住 Liquid Glass 容器"
    );
}

/// glass cursor 使用半透明填充和高对比描边，同时不恢复默认背景整块填充。
///
/// 数据构造（含关键数值推导）：
///   row[0] = 'C'，bg = Background（默认背景）
///   cursor_line=0，cursor_col=0
///   theme.glass_dark().term_bg alpha = 0xfa / 0xff ≈ 98%
///   若 default_bg_fill 恢复为 Some(default_bg)，会再次叠加到接近不透明
///
/// 执行过程：
///   1. build_layout 生成含 glass style 的布局
///   2. cursor_paint_model 从布局派生 cursor 绘制模型
///   3. 检查默认背景仍不整块填充，cursor 自身用半透明 fill + 更强 stroke/highlight
///
/// 预期结果：
///   - 正断言：default_bg_fill == None
///   - 正断言：cursor fill 是 0 到 1 之间的半透明颜色
///   - 正断言：cursor stroke 比 fill 更不透明以保证可见
///   - 负断言：cursor fill 不应是不透明色，否则会变回实心块
#[test]
fn test_glass_cursor_model_keeps_default_background_transparent() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let snapshot = test_snapshot(vec![test_cell('C', fg, bg, CellAttrs::default())]);
    let bounds = Bounds {
        origin: point(px(10.0), px(20.0)),
        size: Size {
            width: px(200.0),
            height: px(100.0),
        },
    };

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let cursor = cursor_paint_model(bounds, &layout);

    assert_eq!(
        layout.default_bg_fill, None,
        "glass cursor 不应通过恢复 default_bg_fill 来制造玻璃感"
    );
    assert!(
        cursor.fill.a > 0.0 && cursor.fill.a < 1.0,
        "glass cursor fill 应是半透明 tint，而不是透明或实心"
    );
    assert!(
        cursor.stroke.a > cursor.fill.a,
        "glass cursor stroke 应比 fill 更不透明，保证深浅背景上可见"
    );
    assert!(
        cursor.highlight.a > 0.0 && cursor.highlight.a < 1.0,
        "glass cursor highlight 应是半透明高光"
    );
    assert_ne!(cursor.fill.a, 1.0, "cursor fill 不应回退为完全不透明实心块");
}

/// cursor bounds 必须严格由 cursor_col/cursor_line 与 cell metrics 推导，避免输入位置错觉。
///
/// 数据构造（含关键数值推导）：
///   bounds.origin = (10, 20)
///   cursor_col = 2，cursor_line = 1
///   cell_width = 8，line_height = 16
///   x = 10 + 2 * 8 = 26
///   y = 20 + 1 * 16 = 36
///   width = 8，height = 16
///
/// 执行过程：
///   1. 构造 2 行 x 3 列快照，cursor 放在第 1 行第 2 列
///   2. build_layout 保留 cursor_line/cursor_col
///   3. cursor_paint_model 计算 cursor bounds
///
/// 预期结果：
///   - 正断言：cursor bounds origin == (26, 36)
///   - 正断言：cursor bounds size == (8, 16)
///   - 负断言：cursor 不应停在 canvas origin，否则输入位置会看起来错误
#[test]
fn test_cursor_bounds_follow_grid_position() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let rows = vec![
        vec![
            test_cell('A', fg, bg, CellAttrs::default()),
            test_cell('B', fg, bg, CellAttrs::default()),
            test_cell('C', fg, bg, CellAttrs::default()),
        ],
        vec![
            test_cell('D', fg, bg, CellAttrs::default()),
            test_cell('E', fg, bg, CellAttrs::default()),
            test_cell('F', fg, bg, CellAttrs::default()),
        ],
    ];
    let snapshot = test_snapshot_with_cursor(rows, 1, 2);
    let bounds = Bounds {
        origin: point(px(10.0), px(20.0)),
        size: Size {
            width: px(200.0),
            height: px(100.0),
        },
    };

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let cursor = cursor_paint_model(bounds, &layout);

    assert_eq!(
        cursor.bounds.origin,
        point(px(26.0), px(36.0)),
        "cursor origin 应为 bounds.origin + (cursor_col * cell_width, cursor_line * line_height)"
    );
    assert_eq!(
        cursor.bounds.size,
        Size {
            width: px(8.0),
            height: px(16.0),
        },
        "cursor size 应等于单个 terminal cell 的尺寸"
    );
    assert_ne!(
        cursor.bounds.origin, bounds.origin,
        "cursor 不应固定在 canvas 原点，否则输入会看起来没有落到真实位置"
    );
}

/// 显式 ANSI 背景、文字和 cursor 同格时，绘制顺序必须保持 background → text → cursor。
///
/// 数据构造（含关键数值推导）：
///   row[0] = 'X'
///   bg = Red（显式 ANSI 背景，不是 default_bg）
///   cursor_line=0，cursor_col=0，与 Red 背景和文字同格
///   期望 layer 数量 = 3：backgrounds、text_runs、cursor
///
/// 执行过程：
///   1. build_layout 解析 Red 背景和 X 文字
///   2. paint_layer_order 从同一个 layout 生成绘制顺序
///   3. 检查 cursor 最后绘制，避免被背景或文字盖掉
///
/// 预期结果：
///   - 正断言：顺序精确等于 [ExplicitBackgrounds, TextRuns, Cursor]
///   - 正断言：最后一层是 Cursor
///   - 负断言：第一层不应是 Cursor，否则光标会被后续背景/文字遮住
#[test]
fn test_paint_layer_order_draws_cursor_after_background_and_text() {
    let palette = test_palette();
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let red_bg = TerminalColor::Named(TerminalNamedColor::Red);
    let snapshot = test_snapshot(vec![test_cell('X', fg, red_bg, CellAttrs::default())]);

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let order = paint_layer_order(&layout);

    assert_eq!(
        order,
        vec![
            TerminalPaintLayer::ExplicitBackgrounds,
            TerminalPaintLayer::TextRuns,
            TerminalPaintLayer::Cursor,
        ],
        "显式背景、文字、cursor 同格时绘制顺序必须保持 background → text → cursor"
    );
    assert_eq!(
        order[2],
        TerminalPaintLayer::Cursor,
        "cursor 必须最后绘制，保证输入位置可见"
    );
    assert_ne!(
        order[0],
        TerminalPaintLayer::Cursor,
        "cursor 不应先绘制，否则会被背景或文字盖掉"
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
