use crate::canvas::text_selection::{SelectionSpan, TextPoint, TextRange};
use crate::cards::term::paint::TerminalRenderMetrics;
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

/// 把 `range`（绝对行坐标）裁剪到当前视口，产出用于渲染高亮的视口相对 spans。
pub fn terminal_selection_spans(snapshot: &GridSnapshot, range: TextRange) -> Vec<SelectionSpan> {
    let viewport_base = snapshot
        .history_size
        .saturating_sub(snapshot.display_offset);
    range
        .spans_for_columns(snapshot.cols)
        .into_iter()
        .filter_map(|span| {
            if span.row < viewport_base {
                return None;
            }
            let viewport_row = span.row - viewport_base;
            if viewport_row >= snapshot.rows.len() {
                return None;
            }
            Some(SelectionSpan::new(
                viewport_row,
                span.start_col,
                span.end_col,
            ))
        })
        .collect()
}

/// 从视口快照中读取选中文本（绝对行须落在当前视口内）。
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

/// 从终端网格读取的绝对行数据构造选中文本。
///
/// `rows[0]` 对应绝对行 `base_abs`；`range` 使用绝对行坐标。复制路径用它拿到
/// 完整 scrollback，而不只是当前视口。
pub fn terminal_text_from_absolute_rows(
    rows: &[Vec<Cell>],
    cols: usize,
    base_abs: usize,
    range: TextRange,
) -> String {
    let normalized = range.normalized();
    let mut lines = Vec::new();
    for abs in normalized.start.row..=normalized.end.row {
        let start_col = if abs == normalized.start.row {
            normalized.start.col
        } else {
            0
        };
        let end_col = if abs == normalized.end.row {
            normalized.end.col
        } else {
            cols
        };
        let text = rows
            .get(abs - base_abs)
            .map(|row| span_text_from_row(row, start_col, end_col))
            .unwrap_or_default();
        lines.push(text);
    }
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
    span_text_from_row(row, span.start_col, span.end_col)
}

fn span_text_from_row(row: &[Cell], start: usize, end: usize) -> String {
    let start = start.min(row.len());
    let end = end.min(row.len());
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
