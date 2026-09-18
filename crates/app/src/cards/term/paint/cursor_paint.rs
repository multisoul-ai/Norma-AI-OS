use gpui::{point, px, quad, transparent_black, Bounds, Edges, Hsla, Pixels, Size, Window};
use terminal::GridSnapshot;

use super::{TerminalLayout, TerminalTextRun};
use crate::cards::term::cursor::{resolve_cursor_style, TerminalCursorAnimation};
use crate::cards::term::paint_style::{resolve_cell_style, TerminalPalette};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalCursorPaint {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) fill: Hsla,
    pub(super) stroke: Hsla,
    pub(super) highlight: Hsla,
    radius: Pixels,
    stroke_width: Pixels,
    highlight_bounds: Bounds<Pixels>,
}

pub(super) fn cursor_text_run(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
) -> Option<TerminalTextRun> {
    if snapshot.display_offset != 0 || !snapshot.show_cursor {
        return None;
    }
    let cell = snapshot
        .rows
        .get(snapshot.cursor_line)?
        .get(snapshot.cursor_col)?;
    let resolved = resolve_cell_style(cell, palette);
    let ch = if cell.c == '\0' { ' ' } else { cell.c };
    if resolved.hidden || resolved.wide_char_spacer || ch == ' ' || ch == '\0' {
        return None;
    }

    let mut style = resolved.text_style();
    style.fg = style.bg;
    Some(TerminalTextRun {
        text: ch.to_string(),
        row: snapshot.cursor_line,
        start_col: snapshot.cursor_col,
        end_col: snapshot.cursor_col + 1,
        style,
    })
}

pub(super) fn cursor_paint_model(
    bounds: Bounds<Pixels>,
    layout: &TerminalLayout,
    cursor_animation: TerminalCursorAnimation,
) -> TerminalCursorPaint {
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

    let cursor_style = resolve_cursor_style(layout.glass_style, cursor_animation);
    TerminalCursorPaint {
        bounds: cursor_bounds,
        fill: cursor_style.fill,
        stroke: cursor_style.stroke,
        highlight: cursor_style.highlight,
        radius: layout.glass_style.cursor_radius,
        stroke_width: layout.glass_style.cursor_stroke_width,
        highlight_bounds,
    }
}

pub(super) fn paint_cursor(cursor: TerminalCursorPaint, window: &mut Window) {
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
