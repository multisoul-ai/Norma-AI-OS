use crate::cards::term::paint::TerminalRenderMetrics;
use crate::text_selection::{SelectionSpan, TextPoint, TextRange};
use gpui::{Pixels, Point};
use terminal::{Cell, GridSnapshot};

pub fn terminal_point_for_local_position(
    position: Point<Pixels>,
    snapshot: &GridSnapshot,
    metrics: TerminalRenderMetrics,
) -> TextPoint {
    let col = grid_axis_index(position.x, metrics.cell_width, snapshot.cols);
    let row_limit = snapshot.lines.saturating_sub(1);
    let row = grid_axis_index(position.y, metrics.line_height, row_limit);
    TextPoint::new(row, col)
}

pub fn terminal_selection_spans(snapshot: &GridSnapshot, range: TextRange) -> Vec<SelectionSpan> {
    range
        .spans_for_columns(snapshot.cols)
        .into_iter()
        .filter(|span| span.row < snapshot.rows.len())
        .collect()
}

pub fn terminal_selected_text(snapshot: &GridSnapshot, range: TextRange) -> String {
    let mut lines = terminal_selection_spans(snapshot, range)
        .into_iter()
        .map(|span| terminal_span_text(snapshot, span))
        .collect::<Vec<_>>();
    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn grid_axis_index(value: Pixels, cell_size: Pixels, max_index: usize) -> usize {
    let cell_size = f32::from(cell_size);
    if cell_size <= 0.0 {
        return 0;
    }
    let raw = (f32::from(value) / cell_size).floor();
    raw.clamp(0.0, max_index as f32) as usize
}

fn terminal_span_text(snapshot: &GridSnapshot, span: SelectionSpan) -> String {
    let Some(row) = snapshot.rows.get(span.row) else {
        return String::new();
    };
    let start = span.start_col.min(row.len());
    let end = span.end_col.min(row.len());
    if start >= end {
        return String::new();
    }

    let mut text = row[start..end]
        .iter()
        .map(copy_char_for_cell)
        .collect::<String>();
    while text.ends_with(' ') {
        text.pop();
    }
    text
}

fn copy_char_for_cell(cell: &Cell) -> char {
    if cell.attrs.hidden || cell.attrs.wide_char_spacer || cell.c == '\0' {
        ' '
    } else {
        cell.c
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/selection_tests.rs"]
mod tests;
