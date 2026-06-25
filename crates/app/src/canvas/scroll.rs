//! Scroll routing shared by canvas and card tests.

use crate::cards::chrome::TITLE_BAR_H;
use crate::cards::{card_for, CardWheelScrollMode};
use canvas::{hit_test, CanvasNode, NodeId, Selection, Vec2, Viewport};

const TERMINAL_SCROLL_PX_PER_LINE: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelScrollOwner {
    CanvasViewport,
    NativeElement(NodeId),
    TerminalScrollback(NodeId),
}

pub(crate) fn wheel_scroll_owner_for_point(
    nodes: &[CanvasNode],
    selection: &Selection,
    screen: Vec2,
    viewport: Viewport,
) -> WheelScrollOwner {
    let canvas_pt = viewport.screen_to_canvas(screen);
    let Some(id) = hit_test(nodes, canvas_pt) else {
        return WheelScrollOwner::CanvasViewport;
    };
    if !selection.contains(id) {
        return WheelScrollOwner::CanvasViewport;
    }
    let Some(node) = nodes.iter().find(|node| node.id == id) else {
        return WheelScrollOwner::CanvasViewport;
    };
    let top_screen = viewport.canvas_to_screen(node.bounds.origin);
    if screen.y <= top_screen.y + TITLE_BAR_H * viewport.zoom {
        return WheelScrollOwner::CanvasViewport;
    }
    match card_for(node.kind.node_type()).wheel_scroll_mode() {
        CardWheelScrollMode::CanvasViewport => WheelScrollOwner::CanvasViewport,
        CardWheelScrollMode::NativeElement => WheelScrollOwner::NativeElement(id),
        CardWheelScrollMode::TerminalScrollback => WheelScrollOwner::TerminalScrollback(id),
    }
}

pub(crate) fn terminal_scroll_lines_from_pixel_delta(delta_y: f32) -> i32 {
    if delta_y == 0.0 {
        return 0;
    }
    let lines = (delta_y.abs() / TERMINAL_SCROLL_PX_PER_LINE).ceil() as i32;
    if delta_y.is_sign_negative() {
        -lines
    } else {
        lines
    }
}

#[cfg(test)]
#[path = "../../tests/unit/card_scroll_tests.rs"]
mod tests;
