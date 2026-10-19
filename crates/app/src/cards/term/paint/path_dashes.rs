//! 项目外文件路径的虚线下划线：布局阶段生成区间，绘制阶段切成短线段。

use super::TerminalRenderMetrics;
use crate::cards::term::path_underline::{dashed_segments, PathMarks};
use gpui::{point, px, quad, transparent_black, Bounds, Edges, Hsla, Pixels, Size, Window};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalDashSpan {
    pub(super) row: usize,
    pub(super) start_col: usize,
    pub(super) end_col: usize,
    pub(super) color: Hsla,
}

pub(super) fn dash_spans(marks: &PathMarks, color: Hsla) -> Vec<TerminalDashSpan> {
    marks
        .dashed_cells()
        .into_iter()
        .map(|(row, start_col, end_col)| TerminalDashSpan {
            row,
            start_col,
            end_col,
            color,
        })
        .collect()
}

/// 虚线段相对单元格宽度的比例：dash 半格、gap 四分之一格（zoom=1 时约 4px / 2px）。
const DASH_CELL_RATIO: f32 = 0.5;
const GAP_CELL_RATIO: f32 = 0.25;
/// 线条距行底的比例（行高 16px 时约 2px），落在文字基线下方。
const BASELINE_INSET_RATIO: f32 = 0.125;

/// 一个虚线段在行内的 `(x 偏移, 宽度)`（像素，相对行起点）与竖直位置。
pub(super) fn dash_geometry(
    span: TerminalDashSpan,
    metrics: TerminalRenderMetrics,
) -> (f32, Vec<(f32, f32)>, f32) {
    let cell = f32::from(metrics.cell_width);
    let width = cell * (span.end_col - span.start_col) as f32;
    let segments = dashed_segments(width, cell * DASH_CELL_RATIO, cell * GAP_CELL_RATIO);
    let line_height = f32::from(metrics.line_height);
    let y = line_height * (span.row as f32 + 1.0) - line_height * BASELINE_INSET_RATIO;
    (cell * span.start_col as f32, segments, y)
}

pub(super) fn paint_dash_span(
    bounds: Bounds<Pixels>,
    span: TerminalDashSpan,
    metrics: TerminalRenderMetrics,
    window: &mut Window,
) {
    let (x0, segments, y) = dash_geometry(span, metrics);
    for (offset, length) in segments {
        let rect = Bounds {
            origin: point(bounds.origin.x + px(x0 + offset), bounds.origin.y + px(y)),
            size: Size {
                width: px(length),
                height: px(1.0),
            },
        };
        window.paint_quad(quad(
            rect,
            px(0.0),
            span.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/cards/term/path_dashes_tests.rs"]
mod tests;
