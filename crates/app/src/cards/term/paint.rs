//! GPUI terminal renderer: resolves neutral terminal snapshot styles into painted runs.

use crate::canvas::text_selection::{SelectionSpan, TextRange};
use crate::cards::term::selection::terminal_selection_spans;
use gpui::{
    canvas, point, prelude::*, px, quad, transparent_black, App, Bounds, Edges, Font, FontFeatures,
    FontStyle, FontWeight, Hsla, IntoElement, Pixels, Point, SharedString, Size,
    StrikethroughStyle, TextRun, UnderlineStyle, Window,
};
use terminal::{Cell, GridSnapshot, TerminalColor, TerminalNamedColor};
use ui::Theme;

use self::cursor_paint::{cursor_paint_model, paint_cursor};
use super::cursor::TerminalCursorAnimation;
use super::paint_style::{
    resolve_cell_style, TerminalGlassStyle, TerminalPalette, TerminalTextStyle,
};

mod cursor_paint;

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
    selection: Option<TextRange>,
    cursor_animation: TerminalCursorAnimation,
) -> impl IntoElement {
    let palette = TerminalPalette::from_theme(theme);
    let glass_style = TerminalGlassStyle::from_theme(theme);
    canvas(
        move |_bounds, _window, _cx| {
            if let Some(selection) = selection {
                build_layout_with_selection(&snapshot, palette, metrics, glass_style, selection)
            } else {
                build_layout(&snapshot, palette, metrics, glass_style)
            }
        },
        move |bounds, layout, window, cx| {
            paint_layout(bounds, &layout, cursor_animation, window, cx)
        },
    )
    .size_full()
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
    selection_spans: Vec<SelectionSpan>,
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
        selection_spans: Vec::new(),
        default_bg: palette.resolve(TerminalColor::Named(TerminalNamedColor::Background)),
        default_bg_fill: None,
        glass_style,
    }
}

fn build_layout_with_selection(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
    selection: TextRange,
) -> TerminalLayout {
    let mut layout = build_layout(snapshot, palette, metrics, glass_style);
    layout.selection_spans = terminal_selection_spans(snapshot, selection)
        .into_iter()
        .filter(|span| !span.is_empty())
        .collect();
    layout
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalPaintLayer {
    DefaultBackgroundFill,
    ExplicitBackgrounds,
    SelectionHighlights,
    TextRuns,
    Cursor,
}

fn paint_layout(
    bounds: Bounds<Pixels>,
    layout: &TerminalLayout,
    cursor_animation: TerminalCursorAnimation,
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
            TerminalPaintLayer::SelectionHighlights => {
                for span in &layout.selection_spans {
                    paint_selection_span(bounds, *span, layout, window);
                }
            }
            TerminalPaintLayer::TextRuns => {
                for run in &layout.text_runs {
                    paint_text_run(bounds, run, metrics, window, cx);
                }
            }
            TerminalPaintLayer::Cursor => {
                paint_cursor(cursor_paint_model(bounds, layout, cursor_animation), window);
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
    if !layout.selection_spans.is_empty() {
        layers.push(TerminalPaintLayer::SelectionHighlights);
    }
    if !layout.text_runs.is_empty() {
        layers.push(TerminalPaintLayer::TextRuns);
    }
    layers.push(TerminalPaintLayer::Cursor);
    layers
}

fn paint_selection_span(
    bounds: Bounds<Pixels>,
    span: SelectionSpan,
    layout: &TerminalLayout,
    window: &mut Window,
) {
    let metrics = layout.metrics;
    let rect = Bounds {
        origin: point(
            bounds.origin.x + metrics.cell_width * (span.start_col as f32),
            bounds.origin.y + metrics.line_height * (span.row as f32),
        ),
        size: Size {
            width: metrics.cell_width * ((span.end_col - span.start_col) as f32),
            height: metrics.line_height,
        },
    };
    window.paint_quad(quad(
        rect,
        px(0.0),
        layout.glass_style.selection_highlight,
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

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/paint_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/paint_geometry_tests.rs"]
mod geometry_tests;
