use super::*;
use terminal::CellAttrs;

fn test_palette() -> TerminalPalette {
    TerminalPalette::from_theme(Theme::glass_dark())
}

fn test_cell(c: char, fg: TerminalColor, bg: TerminalColor, attrs: CellAttrs) -> Cell {
    Cell {
        c,
        fg,
        bg,
        attrs,
        hyperlink: None,
    }
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
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
    }
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
    let cursor = cursor_paint_model(
        bounds,
        &layout,
        crate::cards::term::cursor::TerminalCursorAnimation::static_default(),
    );

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

/// scrollback 回看历史时不应绘制 live cursor，避免历史 viewport 叠加实时输入光标。
///
/// 数据构造（含关键数值推导）：
///   grid = 2 行 × 3 列
///   display_offset = 2，表示当前可见 rows 已经按 line - 2 读取历史内容
///   cursor_line = 1, cursor_col = 2 仍代表 live terminal 底部光标，而不是历史 viewport 光标
///
/// 执行过程：
///   1. 构造带 display_offset=2 的 GridSnapshot
///   2. build_layout 将 snapshot 转成 TerminalLayout
///   3. paint_layer_order 决定实际绘制层
///
/// 预期结果：
///   - 正断言：layout.cursor_visible 为 false，回看历史时不显示 live cursor
///   - 正断言：paint_layer_order 不包含 Cursor 层
///   - 负断言：paint_layer_order 不应继续包含 Cursor，否则历史内容上会叠加错位光标
#[test]
fn test_scrollback_layout_hides_live_cursor() {
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
    let mut snapshot = test_snapshot_with_cursor(rows, 1, 2);
    snapshot.display_offset = 2;
    snapshot.history_size = 2;

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let layers = paint_layer_order(&layout);

    assert!(
        !layout.cursor_visible,
        "display_offset=2 时应隐藏 live cursor，否则历史 viewport 会显示实时输入光标"
    );
    assert!(
        !layers.contains(&TerminalPaintLayer::Cursor),
        "scrollback layout 不应包含 Cursor 绘制层，实际层为 {layers:?}"
    );
    assert_ne!(
        layers.last(),
        Some(&TerminalPaintLayer::Cursor),
        "scrollback 下最后一层不应仍是 Cursor，否则会覆盖历史文本"
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
