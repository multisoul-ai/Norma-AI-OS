//! Terminal layout preparation that runs only on render-cache misses.

use super::layout_build::build_layout_with_marks;
use super::{TerminalLayout, TerminalRenderMetrics, TerminalTextRun};
use crate::canvas::text_selection::TextRange;
use crate::cards::term::paint_style::{TerminalGlassStyle, TerminalPalette};
use crate::cards::term::path_underline::PathMarks;
use gpui::{
    px, Font, FontFeatures, FontStyle, FontWeight, ShapedLine, SharedString, StrikethroughStyle,
    TextRun, UnderlineStyle, Window,
};
use terminal::GridSnapshot;

pub(crate) struct PreparedTerminalTextRun {
    pub(crate) shaped: ShapedLine,
    pub(crate) row: usize,
    pub(crate) start_col: usize,
}

pub(crate) struct PreparedTerminalFrame {
    pub(super) layout: TerminalLayout,
    pub(crate) shaped_text_runs: Vec<PreparedTerminalTextRun>,
    pub(crate) shaped_cursor_text_run: Option<PreparedTerminalTextRun>,
}

pub(super) fn prepare_terminal_frame(
    snapshot: &GridSnapshot,
    palette: TerminalPalette,
    metrics: TerminalRenderMetrics,
    glass_style: TerminalGlassStyle,
    selection: Option<TextRange>,
    marks: &PathMarks,
    window: &mut Window,
) -> PreparedTerminalFrame {
    let layout = build_layout_with_marks(snapshot, palette, metrics, glass_style, selection, marks);
    let shaped_text_runs = layout
        .text_runs
        .iter()
        .map(|run| shape_text_run(run, metrics, window))
        .collect();
    let shaped_cursor_text_run = layout
        .cursor_text_run
        .as_ref()
        .map(|run| shape_text_run(run, metrics, window));
    PreparedTerminalFrame {
        layout,
        shaped_text_runs,
        shaped_cursor_text_run,
    }
}

fn shape_text_run(
    run: &TerminalTextRun,
    metrics: TerminalRenderMetrics,
    window: &mut Window,
) -> PreparedTerminalTextRun {
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
    PreparedTerminalTextRun {
        shaped,
        row: run.row,
        start_col: run.start_col,
    }
}
