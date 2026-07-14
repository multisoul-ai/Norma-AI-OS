use super::*;
use terminal::CellAttrs;

fn test_cell(c: char, fg: TerminalColor, bg: TerminalColor) -> Cell {
    Cell {
        c,
        fg,
        bg,
        attrs: CellAttrs::default(),
        hyperlink: None,
    }
}

fn test_snapshot(row: Vec<Cell>) -> GridSnapshot {
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
    }
}

/// 裸网页链接绘制：识别出的 http(s) 文本应使用链接色和下划线，而普通文本保持原 terminal 样式。
///
/// 数据构造（含关键数值的推导过程）：
///   row 0 = "https://example.com"，19 个 ASCII cells，全部属于同一裸 https URL
///   `GridSnapshot::link_at(0, col)` 对 col=0..18 均返回同一有效目标
///   链接色 = TerminalPalette 的 Cyan；链接下划线 = true
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造没有 OSC 8 metadata 的纯文本快照
///   2. `build_layout` 逐格查询裸 URL，并把相邻同样式链接字符合并为一个 text run
///   3. 读取第一个 text run 的颜色和 underline 状态
///
/// 预期结果：
///   - 断言 A：整段 URL 合并为一个可见 text run，证明裸链接没有被拆散或隐藏
///   - 断言 B：run 使用 Cyan 且 underline=true，提供明确的链接视觉提示
///   - 断言 C：链接色不等于默认 Foreground，避免链接与普通 terminal 文本无法区分
#[test]
fn test_build_layout_emphasizes_bare_https_url_as_underlined_cyan_link() {
    let palette = TerminalPalette::from_theme(Theme::glass_dark());
    let fg = TerminalColor::Named(TerminalNamedColor::Foreground);
    let bg = TerminalColor::Named(TerminalNamedColor::Background);
    let row = "https://example.com"
        .chars()
        .map(|ch| test_cell(ch, fg, bg))
        .collect::<Vec<_>>();
    let snapshot = test_snapshot(row);
    let metrics = TerminalRenderMetrics {
        font_size: px(13.0),
        line_height: px(16.0),
        cell_width: px(8.0),
    };
    let layout = build_layout(
        &snapshot,
        palette,
        metrics,
        TerminalGlassStyle::from_theme(Theme::glass_dark()),
    );
    let run = layout
        .text_runs
        .first()
        .expect("裸 https URL 应生成一个可见 terminal text run");

    assert_eq!(
        run.text, "https://example.com",
        "完整裸 URL 应合并为一个 text run，不应丢字或分裂为普通文本"
    );
    assert_eq!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Cyan)),
        "裸 URL 应使用 Cyan 链接色，便于与默认 Foreground 区分"
    );
    assert!(
        run.style.underline,
        "裸 URL 应带下划线，向用户明确提示可点击性"
    );
    assert_ne!(
        run.style.fg,
        palette.resolve(TerminalColor::Named(TerminalNamedColor::Foreground)),
        "链接色不应退化为默认 Foreground，否则用户无法识别可点击区域"
    );
}
