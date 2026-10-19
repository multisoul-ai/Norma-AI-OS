//! 由终端快照构建一帧布局：文本 run、背景、选区与路径虚线。

use super::cursor_paint::cursor_text_run;
use super::path_dashes::dash_spans;
use super::{layout_row_with_links, TerminalLayout, TerminalRenderMetrics};
use crate::canvas::text_selection::TextRange;
use crate::cards::term::paint_style::{TerminalGlassStyle, TerminalPalette};
use crate::cards::term::path_underline::{LinkMark, PathMarks};
use crate::cards::term::selection::terminal_selection_spans;
use terminal::{GridSnapshot, TerminalColor, TerminalNamedColor};

pub(super) fn build_layout_with_marks(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
    selection: Option<TextRange>,
    marks: &PathMarks,
) -> TerminalLayout {
    let mut backgrounds = Vec::new();
    let mut text_runs = Vec::new();

    for (row_index, row) in snapshot.rows.iter().enumerate() {
        let link_columns = snapshot.link_columns_for_row(row_index);
        let row_layout = layout_row_with_links(row_index, row, palette, |col| {
            if link_columns.get(col).copied().unwrap_or(false) {
                Some(LinkMark::Solid)
            } else {
                marks.mark_at(row_index, col)
            }
        });
        backgrounds.extend(row_layout.backgrounds);
        text_runs.extend(row_layout.text_runs);
    }

    let selection_spans = selection
        .map(|selection| {
            terminal_selection_spans(snapshot, selection)
                .into_iter()
                .filter(|span| !span.is_empty())
                .collect()
        })
        .unwrap_or_default();
    TerminalLayout {
        backgrounds,
        text_runs,
        cursor_text_run: cursor_text_run(snapshot, palette),
        cursor_line: snapshot.cursor_line,
        cursor_col: snapshot.cursor_col,
        cursor_visible: snapshot.display_offset == 0 && snapshot.show_cursor,
        metrics,
        selection_spans,
        default_bg: palette.resolve(TerminalColor::Named(TerminalNamedColor::Background)),
        default_bg_fill: None,
        glass_style,
        path_dashes: dash_spans(
            marks,
            palette.resolve(TerminalColor::Named(TerminalNamedColor::Cyan)),
        ),
    }
}

#[cfg(test)]
pub(super) fn build_layout(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
) -> TerminalLayout {
    build_layout_with_marks(
        snapshot,
        palette,
        metrics,
        glass_style,
        None,
        &PathMarks::default(),
    )
}

#[cfg(test)]
pub(super) fn build_layout_with_selection(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
    selection: TextRange,
) -> TerminalLayout {
    build_layout_with_marks(
        snapshot,
        palette,
        metrics,
        glass_style,
        Some(selection),
        &PathMarks::default(),
    )
}
