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
