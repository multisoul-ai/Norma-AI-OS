use canvas::NodeType;
use gpui::{div, prelude::*, px, rgba, Window};
use runtime::Runtime;
use ui::Theme;

const MAP_WIDTH: f32 = 128.0;
const MAP_HEIGHT: f32 = 112.0;
const MAP_PADDING: f32 = 9.0;
const MAP_FIELD_HEIGHT: f32 = 70.0;

pub(crate) fn render_canvas_minimap(
    runtime: &Runtime,
    theme: Theme,
    window: &Window,
) -> impl IntoElement {
    let viewport_size = window.viewport_size();
    let viewport = runtime.document.viewport;
    let viewport_origin = viewport.screen_to_canvas(canvas::Vec2::ZERO);
    let viewport_width = f32::from(viewport_size.width) / viewport.zoom;
    let viewport_height = f32::from(viewport_size.height) / viewport.zoom;
    let mut min_x = viewport_origin.x;
    let mut min_y = viewport_origin.y;
    let mut max_x = viewport_origin.x + viewport_width;
    let mut max_y = viewport_origin.y + viewport_height;

    for node in runtime.document.nodes() {
        let max = node.bounds.max();
        min_x = min_x.min(node.bounds.origin.x);
        min_y = min_y.min(node.bounds.origin.y);
        max_x = max_x.max(max.x);
        max_y = max_y.max(max.y);
    }

    let world_width = (max_x - min_x).max(1.0);
    let world_height = (max_y - min_y).max(1.0);
    let inner_width = MAP_WIDTH - MAP_PADDING * 2.0;
    let inner_height = MAP_FIELD_HEIGHT - MAP_PADDING * 2.0;
    let scale = (inner_width / world_width).min(inner_height / world_height);
    let offset_x = MAP_PADDING + (inner_width - world_width * scale) * 0.5;
    let offset_y = MAP_PADDING + (inner_height - world_height * scale) * 0.5;

    let mut map = div()
        .id("canvas-minimap")
        .debug_selector(|| "canvas-minimap".to_string())
        .absolute()
        .top(px(74.0))
        .right(px(18.0))
        .w(px(MAP_WIDTH))
        .h(px(MAP_HEIGHT))
        .overflow_hidden()
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x56)))
        .bg(rgba(with_alpha(theme.graphite, 0xb8)));

    for node in runtime.document.nodes() {
        let color = match node.kind.node_type() {
            NodeType::Term => theme.accent_cyan,
            NodeType::Agent => theme.accent_violet,
            NodeType::Preview => theme.accent_green,
            NodeType::Browser => theme.accent_blue,
            NodeType::Rect => theme.text_tertiary,
        };
        let selected = node.selected || runtime.selection.contains(node.id);
        map = map.child(
            div()
                .absolute()
                .left(px(offset_x + (node.bounds.origin.x - min_x) * scale))
                .top(px(offset_y + (node.bounds.origin.y - min_y) * scale))
                .w(px((node.bounds.size.x * scale).max(3.0)))
                .h(px((node.bounds.size.y * scale).max(3.0)))
                .rounded(px(2.0))
                .bg(rgba(with_alpha(color, if selected { 0xf0 } else { 0xa0 }))),
        );
    }

    map = map.child(
        div()
            .absolute()
            .left(px(offset_x + (viewport_origin.x - min_x) * scale))
            .top(px(offset_y + (viewport_origin.y - min_y) * scale))
            .w(px((viewport_width * scale).max(4.0)))
            .h(px((viewport_height * scale).max(4.0)))
            .rounded(px(5.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.accent_blue, 0xe8)))
            .bg(rgba(with_alpha(theme.accent_blue, 0x12))),
    );

    map.child(
        div()
            .absolute()
            .left(px(12.0))
            .right(px(12.0))
            .bottom(px(7.0))
            .h(px(24.0))
            .flex()
            .items_center()
            .justify_between()
            .font_family("Menlo")
            .text_size(px(9.0))
            .text_color(rgba(0xffffffb3))
            .child("−")
            .child(
                div()
                    .id("canvas-minimap-zoom")
                    .debug_selector(|| "canvas-minimap-zoom".to_string())
                    .child(format!("{:.0}%", viewport.zoom * 100.0)),
            )
            .child("＋"),
    )
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
