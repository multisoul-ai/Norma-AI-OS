//! GPUI terminal renderer: resolves neutral terminal snapshot styles into painted runs.

use gpui::{
    canvas, point, prelude::*, px, quad, rgb, rgba, transparent_black, App, Bounds, Edges, Font,
    FontFeatures, FontStyle, FontWeight, Hsla, IntoElement, Pixels, Point, SharedString, Size,
    StrikethroughStyle, TextRun, UnderlineStyle, Window,
};
use terminal::{Cell, GridSnapshot, TerminalColor, TerminalNamedColor};
use ui::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalRenderMetrics {
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub cell_width: Pixels,
}

pub fn render_terminal_canvas(
    snapshot: GridSnapshot,
    theme: Theme,
    metrics: TerminalRenderMetrics,
) -> impl IntoElement {
    let palette = TerminalPalette::from_theme(theme);
    canvas(
        move |_bounds, _window, _cx| build_layout(&snapshot, palette, metrics),
        move |bounds, layout, window, cx| paint_layout(bounds, &layout, window, cx),
    )
    .size_full()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalPalette {
    ansi_colors: [Hsla; 16],
    extended_colors: [Hsla; 256],
    foreground: Hsla,
    background: Hsla,
    cursor: Hsla,
}

impl TerminalPalette {
    fn from_theme(theme: Theme) -> Self {
        let ansi_colors = [
            rgb_to_hsla(0x00, 0x00, 0x00),
            rgb_to_hsla(0xcc, 0x00, 0x00),
            rgb_to_hsla(0x4e, 0x9a, 0x06),
            rgb_to_hsla(0xc4, 0xa0, 0x00),
            rgb_to_hsla(0x34, 0x65, 0xa4),
            rgb_to_hsla(0x75, 0x50, 0x7b),
            rgb_to_hsla(0x06, 0x98, 0x9a),
            rgb_to_hsla(0xd3, 0xd7, 0xcf),
            rgb_to_hsla(0x55, 0x57, 0x53),
            rgb_to_hsla(0xef, 0x29, 0x29),
            rgb_to_hsla(0x8a, 0xe2, 0x34),
            rgb_to_hsla(0xfc, 0xe9, 0x4f),
            rgb_to_hsla(0x72, 0x9f, 0xcf),
            rgb_to_hsla(0xad, 0x7f, 0xa8),
            rgb_to_hsla(0x34, 0xe2, 0xe2),
            rgb_to_hsla(0xee, 0xee, 0xec),
        ];

        let mut extended_colors = [Hsla::default(); 256];
        extended_colors[..16].copy_from_slice(&ansi_colors);

        let mut index = 16;
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    extended_colors[index] = rgb_to_hsla(
                        if r == 0 { 0 } else { 55 + r * 40 },
                        if g == 0 { 0 } else { 55 + g * 40 },
                        if b == 0 { 0 } else { 55 + b * 40 },
                    );
                    index += 1;
                }
            }
        }

        for i in 0..24 {
            let gray = (8 + i * 10) as u8;
            extended_colors[232 + i] = rgb_to_hsla(gray, gray, gray);
        }

        Self {
            ansi_colors,
            extended_colors,
            foreground: rgba(theme.term_fg).into(),
            background: rgba(theme.term_bg).into(),
            cursor: rgba(theme.term_fg).into(),
        }
    }

    fn resolve(self, color: TerminalColor) -> Hsla {
        match color {
            TerminalColor::Named(named) => self.resolve_named(named),
            TerminalColor::Indexed(index) => self.extended_colors[usize::from(index)],
            TerminalColor::Rgb { r, g, b } => rgb_to_hsla(r, g, b),
        }
    }

    fn resolve_named(self, named: TerminalNamedColor) -> Hsla {
        match named {
            TerminalNamedColor::Black => self.ansi_colors[0],
            TerminalNamedColor::Red => self.ansi_colors[1],
            TerminalNamedColor::Green => self.ansi_colors[2],
            TerminalNamedColor::Yellow => self.ansi_colors[3],
            TerminalNamedColor::Blue => self.ansi_colors[4],
            TerminalNamedColor::Magenta => self.ansi_colors[5],
            TerminalNamedColor::Cyan => self.ansi_colors[6],
            TerminalNamedColor::White => self.ansi_colors[7],
            TerminalNamedColor::BrightBlack => self.ansi_colors[8],
            TerminalNamedColor::BrightRed => self.ansi_colors[9],
            TerminalNamedColor::BrightGreen => self.ansi_colors[10],
            TerminalNamedColor::BrightYellow => self.ansi_colors[11],
            TerminalNamedColor::BrightBlue => self.ansi_colors[12],
            TerminalNamedColor::BrightMagenta => self.ansi_colors[13],
            TerminalNamedColor::BrightCyan => self.ansi_colors[14],
            TerminalNamedColor::BrightWhite => self.ansi_colors[15],
            TerminalNamedColor::Foreground => self.foreground,
            TerminalNamedColor::Background => self.background,
            TerminalNamedColor::Cursor => self.cursor,
            TerminalNamedColor::DimBlack => dim_color(self.ansi_colors[0]),
            TerminalNamedColor::DimRed => dim_color(self.ansi_colors[1]),
            TerminalNamedColor::DimGreen => dim_color(self.ansi_colors[2]),
            TerminalNamedColor::DimYellow => dim_color(self.ansi_colors[3]),
            TerminalNamedColor::DimBlue => dim_color(self.ansi_colors[4]),
            TerminalNamedColor::DimMagenta => dim_color(self.ansi_colors[5]),
            TerminalNamedColor::DimCyan => dim_color(self.ansi_colors[6]),
            TerminalNamedColor::DimWhite => dim_color(self.ansi_colors[7]),
            TerminalNamedColor::BrightForeground => brighten_color(self.foreground),
            TerminalNamedColor::DimForeground => dim_color(self.foreground),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedCellStyle {
    fg: Hsla,
    bg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
    hidden: bool,
    wide_char_spacer: bool,
}

impl ResolvedCellStyle {
    fn text_style(self) -> TerminalTextStyle {
        TerminalTextStyle {
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strikeout: self.strikeout,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalTextStyle {
    fg: Hsla,
    bg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalTextRun {
    text: String,
    row: usize,
    start_col: usize,
    end_col: usize,
    style: TerminalTextStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalBackgroundRect {
    row: usize,
    start_col: usize,
    end_col: usize,
    color: Hsla,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TerminalRowLayout {
    backgrounds: Vec<TerminalBackgroundRect>,
    text_runs: Vec<TerminalTextRun>,
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalLayout {
    backgrounds: Vec<TerminalBackgroundRect>,
    text_runs: Vec<TerminalTextRun>,
    cursor_line: usize,
    cursor_col: usize,
    metrics: TerminalRenderMetrics,
    default_bg: Hsla,
    cursor_color: Hsla,
}

fn build_layout(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
) -> TerminalLayout {
    let mut backgrounds = Vec::new();
    let mut text_runs = Vec::new();

    for (row_index, row) in snapshot.rows.iter().enumerate() {
        let row_layout = layout_row(row_index, row, palette);
        backgrounds.extend(row_layout.backgrounds);
        text_runs.extend(row_layout.text_runs);
    }

    TerminalLayout {
        backgrounds,
        text_runs,
        cursor_line: snapshot.cursor_line,
        cursor_col: snapshot.cursor_col,
        metrics,
        default_bg: palette.resolve(TerminalColor::Named(TerminalNamedColor::Background)),
        cursor_color: palette.resolve(TerminalColor::Named(TerminalNamedColor::Cursor)),
    }
}

fn layout_row(row_index: usize, row: &[Cell], palette: TerminalPalette) -> TerminalRowLayout {
    let mut layout = TerminalRowLayout::default();
    let mut current_bg: Option<TerminalBackgroundRect> = None;
    let mut current_text: Option<TerminalTextRun> = None;

    for (col, cell) in row.iter().enumerate() {
        let style = resolve_cell_style(cell, palette);
        push_background(
            &mut layout.backgrounds,
            &mut current_bg,
            row_index,
            col,
            style.bg,
        );

        let ch = if cell.c == '\0' { ' ' } else { cell.c };
        let has_visible_text = !style.hidden && !style.wide_char_spacer && ch != ' ' && ch != '\0';
        if !has_visible_text {
            flush_text(&mut layout.text_runs, &mut current_text);
            continue;
        }

        let text_style = style.text_style();
        match current_text.as_mut() {
            Some(run) if run.style == text_style && run.end_col == col => {
                run.text.push(ch);
                run.end_col = col + 1;
            }
            Some(_) => {
                flush_text(&mut layout.text_runs, &mut current_text);
                current_text = Some(TerminalTextRun {
                    text: ch.to_string(),
                    row: row_index,
                    start_col: col,
                    end_col: col + 1,
                    style: text_style,
                });
            }
            None => {
                current_text = Some(TerminalTextRun {
                    text: ch.to_string(),
                    row: row_index,
                    start_col: col,
                    end_col: col + 1,
                    style: text_style,
                });
            }
        }
    }

    flush_background(&mut layout.backgrounds, &mut current_bg);
    flush_text(&mut layout.text_runs, &mut current_text);
    layout
}

fn push_background(
    backgrounds: &mut Vec<TerminalBackgroundRect>,
    current_bg: &mut Option<TerminalBackgroundRect>,
    row: usize,
    col: usize,
    color: Hsla,
) {
    match current_bg.as_mut() {
        Some(bg) if bg.color == color && bg.end_col == col => {
            bg.end_col = col + 1;
        }
        Some(_) => {
            flush_background(backgrounds, current_bg);
            *current_bg = Some(TerminalBackgroundRect {
                row,
                start_col: col,
                end_col: col + 1,
                color,
            });
        }
        None => {
            *current_bg = Some(TerminalBackgroundRect {
                row,
                start_col: col,
                end_col: col + 1,
                color,
            });
        }
    }
}

fn flush_background(
    backgrounds: &mut Vec<TerminalBackgroundRect>,
    current_bg: &mut Option<TerminalBackgroundRect>,
) {
    if let Some(bg) = current_bg.take() {
        backgrounds.push(bg);
    }
}

fn flush_text(text_runs: &mut Vec<TerminalTextRun>, current_text: &mut Option<TerminalTextRun>) {
    if let Some(run) = current_text.take() {
        text_runs.push(run);
    }
}

fn resolve_cell_style(cell: &Cell, palette: TerminalPalette) -> ResolvedCellStyle {
    let mut fg = palette.resolve(cell.fg);
    let mut bg = palette.resolve(cell.bg);

    if cell.attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    if cell.attrs.dim {
        fg = dim_color(fg);
    }

    ResolvedCellStyle {
        fg,
        bg,
        bold: cell.attrs.bold,
        italic: cell.attrs.italic,
        underline: cell.attrs.underline,
        strikeout: cell.attrs.strikeout,
        hidden: cell.attrs.hidden,
        wide_char_spacer: cell.attrs.wide_char_spacer,
    }
}

fn paint_layout(
    bounds: Bounds<Pixels>,
    layout: &TerminalLayout,
    window: &mut Window,
    cx: &mut App,
) {
    let metrics = layout.metrics;

    window.paint_quad(quad(
        bounds,
        px(0.0),
        layout.default_bg,
        Edges::<Pixels>::default(),
        transparent_black(),
        Default::default(),
    ));

    for bg in &layout.backgrounds {
        if bg.color == layout.default_bg {
            continue;
        }
        let rect = Bounds {
            origin: point(
                bounds.origin.x + metrics.cell_width * (bg.start_col as f32),
                bounds.origin.y + metrics.line_height * (bg.row as f32),
            ),
            size: Size {
                width: metrics.cell_width * ((bg.end_col - bg.start_col) as f32),
                height: metrics.line_height,
            },
        };
        window.paint_quad(quad(
            rect,
            px(0.0),
            bg.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }

    for run in &layout.text_runs {
        paint_text_run(bounds, run, metrics, window, cx);
    }

    let cursor_bounds = Bounds {
        origin: point(
            bounds.origin.x + metrics.cell_width * (layout.cursor_col as f32),
            bounds.origin.y + metrics.line_height * (layout.cursor_line as f32),
        ),
        size: Size {
            width: metrics.cell_width,
            height: metrics.line_height,
        },
    };
    window.paint_quad(quad(
        cursor_bounds,
        px(0.0),
        layout.cursor_color,
        Edges::<Pixels>::default(),
        transparent_black(),
        Default::default(),
    ));
}

fn paint_text_run(
    bounds: Bounds<Pixels>,
    run: &TerminalTextRun,
    metrics: TerminalRenderMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    let font = Font {
        family: "Menlo".into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: if run.style.bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        },
        style: if run.style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
    };
    let underline = run.style.underline.then_some(UnderlineStyle {
        thickness: px(1.0),
        color: Some(run.style.fg),
        wavy: false,
    });
    let strikethrough = run.style.strikeout.then_some(StrikethroughStyle {
        thickness: px(1.0),
        color: Some(run.style.fg),
    });
    let text_run = TextRun {
        len: run.text.len(),
        font,
        color: run.style.fg,
        background_color: None,
        underline,
        strikethrough,
    };
    let text: SharedString = run.text.clone().into();
    let shaped = window
        .text_system()
        .shape_line(text, metrics.font_size, &[text_run], None);
    let origin = text_run_origin(bounds, run, metrics);
    let _ = shaped.paint(origin, metrics.line_height, window, cx);
}

fn text_run_origin(
    bounds: Bounds<Pixels>,
    run: &TerminalTextRun,
    metrics: TerminalRenderMetrics,
) -> Point<Pixels> {
    point(
        bounds.origin.x + metrics.cell_width * (run.start_col as f32),
        bounds.origin.y + metrics.line_height * (run.row as f32) + vertical_text_offset(metrics),
    )
}

fn vertical_text_offset(metrics: TerminalRenderMetrics) -> Pixels {
    px(((f32::from(metrics.line_height) - f32::from(metrics.font_size)) / 2.0).max(0.0))
}

fn rgb_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into()
}

fn dim_color(mut color: Hsla) -> Hsla {
    color.l *= 0.7;
    color
}

fn brighten_color(mut color: Hsla) -> Hsla {
    color.l = (color.l * 1.2).min(1.0);
    color
}

#[cfg(test)]
mod tests {
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
}
