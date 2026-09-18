use super::*;
use terminal::CellAttrs;

fn test_palette() -> TerminalPalette {
    TerminalPalette::from_theme(Theme::glass_dark())
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

fn test_cell(c: char, fg: TerminalNamedColor) -> Cell {
    Cell {
        c,
        fg: TerminalColor::Named(fg),
        bg: TerminalColor::Named(TerminalNamedColor::Background),
        attrs: CellAttrs::default(),
        hyperlink: None,
    }
}

fn suggestion_snapshot(cursor_col: usize) -> GridSnapshot {
    let rows = vec![vec![
        test_cell('g', TerminalNamedColor::Foreground),
        test_cell('e', TerminalNamedColor::BrightBlack),
        test_cell('x', TerminalNamedColor::BrightBlack),
        test_cell('i', TerminalNamedColor::BrightBlack),
        test_cell('t', TerminalNamedColor::BrightBlack),
    ]];
    GridSnapshot {
        rows,
        cursor_line: 0,
        cursor_col,
        cols: 5,
        lines: 1,
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
        show_cursor: true,
    }
}

/// 光标位于灰色 autosuggestion 首字符时，必须在 cursor 之后反色重绘该字符。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = "gexit" = 5 cells
///   typed text = col 0 的前景色字符 'g'
///   suggestion = cols 1..5 的 BrightBlack 文本 "exit"
///   cursor_col = 1，恰好与 suggestion 首字符 'e' 同格
///   expected layers = TextRuns(1) + Cursor(1) + CursorText(1) = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `build_layout` 把 BrightBlack 建议保留为普通 text run。
///   2. 同一布局识别 cursor cell 的可见字符 'e'，生成单 cell cursor text run。
///   3. cursor text 使用 cell 背景色作为前景色，实现终端 block cursor 的反色语义。
///   4. `paint_layer_order` 在普通文字后绘制 cursor，再在最上层重绘 cursor text。
///
/// 预期结果：
///   - 正断言：cursor text 精确为 col 1 的 'e'，并使用 Background 反色前景。
///   - 正断言：绘制顺序精确为 TextRuns → Cursor → CursorText，建议字符不会被盖住。
///   - 负断言：cursor text 不得继续使用 BrightBlack，否则绿色填充会吞掉灰字。
#[test]
fn test_cursor_repaints_gray_autosuggestion_first_character_after_cursor() {
    let palette = test_palette();
    let snapshot = suggestion_snapshot(1);

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let cursor_text = layout
        .cursor_text_run
        .as_ref()
        .expect("光标位于灰色建议首字符 'e' 时必须生成 cursor text run");
    let order = paint_layer_order(&layout);

    assert_eq!(
        cursor_text.text, "e",
        "cursor text 必须只重绘建议首字符 'e'"
    );
    assert_eq!(
        (cursor_text.row, cursor_text.start_col, cursor_text.end_col),
        (0, 1, 2),
        "cursor text 必须精确覆盖 row 0 的 col 1 单个 cell"
    );
    assert_eq!(
        cursor_text.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Background)),
        "block cursor 内的 'e' 必须使用 cell 背景色作为反色前景"
    );
    assert_ne!(
        cursor_text.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::BrightBlack)),
        "cursor text 不得保留 BrightBlack，否则灰字仍会被 cursor fill 吞掉"
    );
    assert_eq!(
        order,
        vec![
            TerminalPaintLayer::TextRuns,
            TerminalPaintLayer::Cursor,
            TerminalPaintLayer::CursorText,
        ],
        "灰色建议与 cursor 同格时必须按 TextRuns → Cursor → CursorText 合成"
    );
    assert_ne!(
        order.last(),
        Some(&TerminalPaintLayer::Cursor),
        "cursor 不得成为最后一层并盖住 autosuggestion 首字符"
    );
}

/// 光标位于空白 cell 时仍只绘制原有 cursor，不制造伪字符层。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = "g " = 2 cells
///   visible text runs = 1（仅 'g'）
///   cursor_col = 1，cell 字符为空格
///   expected layers = TextRuns(1) + Cursor(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `build_layout` 生成 'g' 的普通 text run。
///   2. cursor cell 是空格，不生成 cursor text run。
///   3. `paint_layer_order` 保持现有 TextRuns → Cursor 顺序。
///
/// 预期结果：
///   - 正断言：空白位置仍保留 Cursor 层。
///   - 正断言：绘制顺序精确为 TextRuns → Cursor。
///   - 负断言：不得生成 CursorText，否则空白光标会出现伪字符。
#[test]
fn test_cursor_on_blank_cell_keeps_existing_cursor_without_text_repaint() {
    let palette = test_palette();
    let mut snapshot = suggestion_snapshot(1);
    snapshot.rows[0] = vec![
        test_cell('g', TerminalNamedColor::Foreground),
        test_cell(' ', TerminalNamedColor::Foreground),
    ];
    snapshot.cols = 2;

    let layout = build_layout(&snapshot, palette, test_metrics(), test_glass_style());
    let order = paint_layer_order(&layout);

    assert!(
        layout.cursor_text_run.is_none(),
        "空白 cursor cell 不应生成 cursor text run"
    );
    assert_eq!(
        order,
        vec![TerminalPaintLayer::TextRuns, TerminalPaintLayer::Cursor],
        "空白 cell 的现有光标顺序必须保持 TextRuns → Cursor"
    );
    assert!(
        order.contains(&TerminalPaintLayer::Cursor),
        "空白 cursor cell 仍必须显示原有 Cursor 层"
    );
    assert!(
        !order.contains(&TerminalPaintLayer::CursorText),
        "空白 cursor cell 不得绘制 CursorText 伪字符层"
    );
}
