use crate::app::AppScreen;
use crate::cards::chrome::TITLE_BAR_H;
use crate::cards::term::view::{TERM_CELL_PX, TERM_LINE_PX, TERM_PADDING_PX};
use gpui::{point, px, Bounds, Pixels, Size};
use runtime::{InputMode, NodeView, Runtime};

pub(crate) fn terminal_ime_cursor_bounds(
    runtime: &Runtime,
    screen: AppScreen,
) -> Option<Bounds<Pixels>> {
    if screen != AppScreen::Canvas {
        return None;
    }

    let InputMode::NodeFocused(node_id) = runtime.focus.mode() else {
        return None;
    };
    let node = runtime.document.node(node_id)?;
    let NodeView::Term { snapshot, .. } = runtime.node_view(node_id)?;
    let zoom = runtime.document.viewport.zoom;
    let node_top_left = runtime
        .document
        .viewport
        .canvas_to_screen(node.bounds.origin);
    let cursor_col = snapshot.cursor_col.min(snapshot.cols.saturating_sub(1));
    let cursor_line = snapshot.cursor_line.min(snapshot.lines.saturating_sub(1));

    Some(Bounds {
        origin: point(
            px(node_top_left.x + (TERM_PADDING_PX + TERM_CELL_PX * cursor_col as f32) * zoom),
            px(node_top_left.y
                + (TITLE_BAR_H + TERM_PADDING_PX + TERM_LINE_PX * cursor_line as f32) * zoom),
        ),
        size: Size {
            width: px(TERM_CELL_PX * zoom),
            height: px(TERM_LINE_PX * zoom),
        },
    })
}

#[cfg(test)]
#[path = "../../tests/unit/app_ime_geometry_tests.rs"]
mod tests;
