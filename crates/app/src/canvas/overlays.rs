use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Window};

use super::terminal_link::render_terminal_link_tooltip;
use crate::app::MultisoulApp;
use crate::cards::agent::queue::{render_panel as render_agent_queue_panel, AgentQueuePanelLayout};

/// 绘制必须位于局部 Working Snake 之上的 Canvas 交互叠层。
pub(crate) fn render_canvas_overlays(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let viewport = app.runtime.document.viewport;
    let window_size = window.viewport_size();
    let window_width = f32::from(window_size.width);
    let window_height = f32::from(window_size.height);
    let mut overlays = Vec::new();

    if let Some(link) = app.interaction.hovered_terminal_link.clone() {
        overlays.push(render_terminal_link_tooltip(
            link,
            app.theme,
            window_width,
            window_height,
        ));
    }
    if let Some(node_id) = app.agent_queue_ui.open_node_id() {
        if let Some((card_left, card_top, card_width)) =
            app.runtime.document.node(node_id).map(|node| {
                let top_left = viewport.canvas_to_screen(node.bounds.origin);
                (top_left.x, top_left.y, node.bounds.size.x * viewport.zoom)
            })
        {
            if let Some(panel) = render_agent_queue_panel(
                app,
                node_id,
                AgentQueuePanelLayout {
                    card_left,
                    card_top,
                    card_width,
                    window_width,
                    window_height,
                },
                window,
                cx,
            ) {
                overlays.push(panel);
            }
        }
    }

    div()
        .id("canvas-overlays")
        .absolute()
        .size_full()
        .children(overlays)
}

pub(super) fn swap_highlight_overlay(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius_px: f32,
    color: u32,
) -> AnyElement {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .border_2()
        .border_color(rgba(color))
        .rounded(px(radius_px))
        .into_any_element()
}
