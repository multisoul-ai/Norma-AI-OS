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
    let glass_style = TerminalGlassStyle::from_theme(theme);
    canvas(
        move |_bounds, _window, _cx| build_layout(&snapshot, palette, metrics, glass_style),
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
struct TerminalGlassStyle {
    cursor_fill: Hsla,
    cursor_stroke: Hsla,
    cursor_highlight: Hsla,
    cursor_radius: Pixels,
    cursor_stroke_width: Pixels,
    cursor_highlight_height: Pixels,
}

impl TerminalGlassStyle {
    fn from_theme(theme: Theme) -> Self {
        let accent: Hsla = rgba(theme.focused_stroke).into();
        let highlight: Hsla = rgba(theme.edge_highlight).into();
        Self {
            cursor_fill: hsla_with_alpha(accent, 0.42),
            cursor_stroke: hsla_with_alpha(accent, 0.86),
            cursor_highlight: hsla_with_alpha(highlight, 0.55),
            cursor_radius: px(3.0),
            cursor_stroke_width: px(1.0),
            cursor_highlight_height: px(2.0),
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
    default_bg_fill: Option<Hsla>,
    glass_style: TerminalGlassStyle,
}

fn build_layout(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
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
        default_bg_fill: None,
        glass_style,
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalCursorPaint {
    bounds: Bounds<Pixels>,
    fill: Hsla,
    stroke: Hsla,
    highlight: Hsla,
    radius: Pixels,
    stroke_width: Pixels,
    highlight_bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalPaintLayer {
    DefaultBackgroundFill,
    ExplicitBackgrounds,
    TextRuns,
    Cursor,
}

fn paint_layout(
    bounds: Bounds<Pixels>,
    layout: &TerminalLayout,
    window: &mut Window,
    cx: &mut App,
) {
    let metrics = layout.metrics;

    for layer in paint_layer_order(layout) {
        match layer {
            TerminalPaintLayer::DefaultBackgroundFill => {
                if let Some(default_bg_fill) = layout.default_bg_fill {
                    window.paint_quad(quad(
                        bounds,
                        px(0.0),
                        default_bg_fill,
                        Edges::<Pixels>::default(),
                        transparent_black(),
                        Default::default(),
                    ));
                }
            }
            TerminalPaintLayer::ExplicitBackgrounds => {
                for bg in paintable_backgrounds(layout) {
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
            }
            TerminalPaintLayer::TextRuns => {
                for run in &layout.text_runs {
                    paint_text_run(bounds, run, metrics, window, cx);
                }
            }
            TerminalPaintLayer::Cursor => {
                paint_cursor(cursor_paint_model(bounds, layout), window);
            }
        }
    }
}

fn paintable_backgrounds(
    layout: &TerminalLayout,
) -> impl Iterator<Item = TerminalBackgroundRect> + '_ {
    layout
        .backgrounds
        .iter()
        .copied()
        .filter(|bg| bg.color != layout.default_bg)
}

fn paint_layer_order(layout: &TerminalLayout) -> Vec<TerminalPaintLayer> {
    let mut layers = Vec::new();
    if layout.default_bg_fill.is_some() {
        layers.push(TerminalPaintLayer::DefaultBackgroundFill);
    }
    if paintable_backgrounds(layout).next().is_some() {
        layers.push(TerminalPaintLayer::ExplicitBackgrounds);
    }
    if !layout.text_runs.is_empty() {
        layers.push(TerminalPaintLayer::TextRuns);
    }
    layers.push(TerminalPaintLayer::Cursor);
    layers
}

fn cursor_paint_model(bounds: Bounds<Pixels>, layout: &TerminalLayout) -> TerminalCursorPaint {
    let metrics = layout.metrics;
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
    let inset = layout.glass_style.cursor_stroke_width;
    let highlight_width = px((f32::from(metrics.cell_width) - f32::from(inset) * 2.0).max(0.0));
    let highlight_height = px(f32::from(layout.glass_style.cursor_highlight_height)
        .min(f32::from(metrics.line_height))
        .max(0.0));
    let highlight_bounds = Bounds {
        origin: point(
            cursor_bounds.origin.x + inset,
            cursor_bounds.origin.y + inset,
        ),
        size: Size {
            width: highlight_width,
            height: highlight_height,
        },
    };

    TerminalCursorPaint {
        bounds: cursor_bounds,
        fill: layout.glass_style.cursor_fill,
        stroke: layout.glass_style.cursor_stroke,
        highlight: layout.glass_style.cursor_highlight,
        radius: layout.glass_style.cursor_radius,
        stroke_width: layout.glass_style.cursor_stroke_width,
        highlight_bounds,
    }
}

fn paint_cursor(cursor: TerminalCursorPaint, window: &mut Window) {
    window.paint_quad(quad(
        cursor.bounds,
        cursor.radius,
        cursor.fill,
        Edges::all(cursor.stroke_width),
        cursor.stroke,
        Default::default(),
    ));

    if f32::from(cursor.highlight_bounds.size.width) > 0.0
        && f32::from(cursor.highlight_bounds.size.height) > 0.0
    {
        window.paint_quad(quad(
            cursor.highlight_bounds,
            cursor.radius,
            cursor.highlight,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
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

fn hsla_with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha.clamp(0.0, 1.0);
    color
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
#[path = "paint_tests.rs"]
mod tests;
