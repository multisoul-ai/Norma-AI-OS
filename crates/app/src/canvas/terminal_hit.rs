use crate::app::MultisoulApp;
use crate::canvas::geometry::terminal_content_local_position;
use crate::cards::term::selection::terminal_point_for_local_position;
use crate::cards::term::view::terminal_render_metrics;
use canvas::{NodeId, Vec2};
use runtime::NodeView;

use super::text_selection::TextPoint;

pub(crate) fn terminal_text_point_for_screen(
    app: &MultisoulApp,
    node_id: NodeId,
    screen: Vec2,
) -> Option<TextPoint> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let local = terminal_content_local_position(screen, node_top_left, viewport.zoom);
    Some(terminal_point_for_local_position(
        local,
        &snapshot,
        terminal_render_metrics(viewport.zoom),
    ))
}

/// 链接命中专用坐标换算：只接受真实终端网格内容区域，不能把 padding 钳制为边缘 cell。
pub(crate) fn terminal_link_point_for_screen(
    app: &MultisoulApp,
    node_id: NodeId,
    screen: Vec2,
) -> Option<TextPoint> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let local = terminal_content_local_position(screen, node_top_left, viewport.zoom);
    let metrics = terminal_render_metrics(viewport.zoom);
    let x = f32::from(local.x);
    let y = f32::from(local.y);
    let width = f32::from(metrics.cell_width) * snapshot.cols as f32;
    let height = f32::from(metrics.line_height) * snapshot.lines as f32;
    if x < 0.0 || y < 0.0 || x >= width || y >= height {
        return None;
    }
    Some(terminal_point_for_local_position(local, &snapshot, metrics))
}
