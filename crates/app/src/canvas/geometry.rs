use crate::cards::chrome::TITLE_BAR_H;
use crate::cards::term::view::TERM_PADDING_PX;
use canvas::{NodeId, Vec2, Viewport};
use gpui::{point, px, Pixels, Point};
use runtime::InputEvent;

pub(crate) fn node_card_radius_px(zoom: f32) -> f32 {
    (22.0 * zoom).clamp(12.0, 28.0)
}

pub(crate) fn node_title_radius_px(zoom: f32) -> f32 {
    node_card_radius_px(zoom)
}

pub(crate) fn node_terminal_radius_px(zoom: f32) -> f32 {
    node_card_radius_px(zoom)
}

pub(crate) fn pt_to_vec(p: Point<Pixels>) -> Vec2 {
    Vec2::new(f32::from(p.x), f32::from(p.y))
}

pub(crate) fn terminal_content_local_position(
    screen: Vec2,
    node_top_left: Vec2,
    zoom: f32,
) -> Point<Pixels> {
    point(
        px(screen.x - node_top_left.x - TERM_PADDING_PX),
        px(screen.y - node_top_left.y - TITLE_BAR_H * zoom - TERM_PADDING_PX),
    )
}

pub(crate) fn is_title_bar_hit(screen_y: f32, node_top_screen_y: f32, zoom: f32) -> bool {
    screen_y <= node_top_screen_y + TITLE_BAR_H * zoom
}

pub(crate) fn click_event_for_node(
    hit: Option<NodeId>,
    is_focusable: bool,
    is_title_bar: bool,
    click_count: usize,
) -> InputEvent {
    if click_count >= 2 || (is_focusable && !is_title_bar) {
        InputEvent::DoubleClick { hit, is_focusable }
    } else {
        InputEvent::Click { hit }
    }
}

pub(crate) fn apply_canvas_wheel_scroll(
    viewport: &mut Viewport,
    pixel_delta: Vec2,
    enabled: bool,
) -> bool {
    if !enabled || (pixel_delta.x == 0.0 && pixel_delta.y == 0.0) {
        return false;
    }
    viewport.pan = Vec2::new(
        viewport.pan.x - pixel_delta.x,
        viewport.pan.y - pixel_delta.y,
    );
    true
}
