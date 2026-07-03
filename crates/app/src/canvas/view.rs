//! 画布节点渲染：viewport culling、标题栏、内容体。
//! 事件处理（pan、拖拽、单击）见 `events` 子模块。

use super::events;
use crate::app::MultisoulApp;
#[cfg(test)]
use crate::canvas::events::{should_focus_app_root_for_mouse_down, should_notify_motion_frame};
#[cfg(test)]
use crate::canvas::geometry::terminal_content_local_position;
#[cfg(test)]
use crate::canvas::geometry::{apply_canvas_wheel_scroll, click_event_for_node, is_title_bar_hit};
use crate::canvas::geometry::{
    node_card_radius_px, node_edge_highlight_inset_px, node_terminal_radius_px,
    node_title_radius_px,
};
#[cfg(test)]
use crate::canvas::interaction::InteractionState;
use crate::cards::agent::card::{agent_breathing_intent, agent_chase_intent};
use crate::cards::chrome::{
    node_shadow, render_task_badge, CORNER_GLOW_MIN_SCREEN_PX, TITLE_BAR_H,
};
use crate::cards::snake::render_working_snake_border;
use crate::cards::{card_for, CardBodyContext};
#[cfg(test)]
use canvas::Vec2;
use canvas::{NodeId, NodeType};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, MouseButton, Window};
use runtime::InputMode;
#[cfg(test)]
use runtime::{route, Route};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static CANVAS_OPEN_RENDER_PENDING: AtomicBool = AtomicBool::new(false);

pub(crate) fn mark_canvas_open_render_pending() {
    CANVAS_OPEN_RENDER_PENDING.store(true, Ordering::Release);
}

pub fn render_canvas(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let render_start = Instant::now();
    let total_nodes = app.runtime.document.nodes().len();
    let _span = tracing::trace_span!("render_canvas", nodes = total_nodes).entered();

    let theme = app.theme;
    let vp = app.runtime.document.viewport;
    let win = window.viewport_size();
    let (win_w, win_h) = (f32::from(win.width), f32::from(win.height));
    let focus_mode = app.runtime.focus.mode();

    // 先收集需要渲染的节点数据（避免借用冲突）。
    let mut node_elems: Vec<AnyElement> = Vec::new();
    let mut visible_nodes = 0usize;
    let node_ids: Vec<NodeId> = app.runtime.document.nodes().iter().map(|n| n.id).collect();
    app.motion.retain_nodes(node_ids.iter().copied());
    for id in node_ids {
        let Some((canonical_rect, selected, node_type)) =
            app.runtime.document.node(id).map(|node| {
                (
                    node.bounds,
                    node.selected || app.runtime.selection.contains(id),
                    node.kind.node_type(),
                )
            })
        else {
            continue;
        };
        let focused = matches!(focus_mode, InputMode::NodeFocused(fid) if fid == id);
        app.motion.ensure_node(id, canonical_rect);
        app.motion.set_selection_focus(id, selected, focused);
        let is_agent = node_type == NodeType::Agent;
        let agent_state = is_agent.then(|| app.runtime.agent_task_state(id)).flatten();
        let card_attention = app.runtime.card_attention(id);
        let breathing_intent = agent_breathing_intent(agent_state.clone(), card_attention);
        app.motion.set_breathing_intent(id, breathing_intent);
        app.motion
            .set_chase_intent(id, is_agent && agent_chase_intent(agent_state));
        if app
            .interaction
            .drag
            .map(|drag| drag.node == id)
            .unwrap_or(false)
        {
            app.motion.sync_drag_rect(id, canonical_rect);
        }

        let visual = app.motion.resolve_node_visual(id, canonical_rect, theme);
        let top_left = vp.canvas_to_screen(visual.rect.origin);
        let (w, h) = (visual.rect.size.x * vp.zoom, visual.rect.size.y * vp.zoom);

        // viewport culling：完全在窗口外的节点跳过。
        if top_left.x + w < 0.0 || top_left.y + h < 0.0 || top_left.x > win_w || top_left.y > win_h
        {
            continue;
        }
        visible_nodes += 1;

        let node = app.runtime.document.node(id).unwrap().clone();
        let card = card_for(node_type);
        let chrome = card.chrome(&node, &app.runtime);
        let title = chrome.title;
        let task_badge = chrome.task_badge;
        let refreshable = chrome.refreshable;
        let stroke = visual.stroke_color;
        let status = visual.status_color;
        let close_button_size = (18.0 * vp.zoom).clamp(16.0, 24.0);
        let node_radius = node_card_radius_px(vp.zoom);

        let mut node_div = div()
            .id(("node-card", id))
            .absolute()
            .left(px(top_left.x))
            .top(px(top_left.y))
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .shadow(node_shadow(
                visual,
                vp.zoom,
                w >= CORNER_GLOW_MIN_SCREEN_PX && h >= CORNER_GLOW_MIN_SCREEN_PX,
            ))
            .border_color(rgba(stroke))
            .bg(rgba(visual.fill_color))
            .opacity(visual.opacity)
            .rounded(px(node_radius))
            .overflow_hidden()
            .on_hover(cx.listener(move |app, hovered: &bool, _window, cx| {
                app.motion.set_hover(id, *hovered);
                cx.notify();
            }));

        node_div = if visual.border_is_emphasized {
            node_div.border_2()
        } else {
            node_div.border_1()
        };

        let edge_highlight = div()
            .absolute()
            .top(px(0.0))
            .left(px(node_edge_highlight_inset_px(vp.zoom)))
            .right(px(node_edge_highlight_inset_px(vp.zoom)))
            .h(px(1.0))
            .bg(rgba(visual.edge_highlight_color));

        // 标题栏
        let mut title_actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0 * vp.zoom));
        if refreshable {
            title_actions = title_actions.child(
                div()
                    .id(("node-refresh", id))
                    .w(px(close_button_size))
                    .h(px(close_button_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px((6.0 * vp.zoom).clamp(5.0, 8.0)))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x12)))
                    .text_color(rgba(theme.text_tertiary))
                    .text_size(px((12.0 * vp.zoom).clamp(10.0, 13.0)))
                    .child("R")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _window, cx| {
                        cx.stop_propagation();
                        app.refresh_preview_node(id, cx);
                        cx.notify();
                    })),
            );
        }
        title_actions = title_actions.child(
            div()
                .id(("node-close", id))
                .w(px(close_button_size))
                .h(px(close_button_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px((6.0 * vp.zoom).clamp(5.0, 8.0)))
                .bg(rgba(with_alpha(theme.destructive, 0x12)))
                .text_color(rgba(theme.text_tertiary))
                .text_size(px((12.0 * vp.zoom).clamp(10.0, 13.0)))
                .child("x")
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.close_node_through_route(id, window);
                    cx.notify();
                })),
        );

        let title_bar = div()
            .h(px(TITLE_BAR_H * vp.zoom))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .rounded_t(px(node_title_radius_px(vp.zoom)))
            .bg(rgba(visual.title_bar_color))
            .text_color(rgba(theme.text_primary))
            .text_size(px((12.0 * vp.zoom).max(9.0)))
            .px(px(10.0 * vp.zoom))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .gap_2()
                    .child(
                        div()
                            .w(px(7.0 * vp.zoom))
                            .h(px(7.0 * vp.zoom))
                            .flex_none()
                            .rounded(px(4.0 * vp.zoom))
                            .bg(rgba(status)),
                    )
                    .child(
                        div()
                            .id(("node-title", id))
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .truncate()
                            .child(title),
                    )
                    .when_some(task_badge, |this, badge| {
                        this.child(render_task_badge(badge, vp.zoom, theme))
                    }),
            )
            .child(title_actions);

        // 内容
        let card_body = card.body(
            &node,
            CardBodyContext {
                runtime: &app.runtime,
                preview_state: app.previews.state_for(id),
                text_selection: app.interaction.active_text_selection,
                focused,
                reduce_motion: app.motion.reduce_motion,
                terminal_cursor_phase: app.terminal_cursor_phase,
                theme,
                zoom: vp.zoom,
                bottom_radius_px: node_terminal_radius_px(vp.zoom),
            },
        );
        node_div = node_div
            .child(edge_highlight)
            .child(title_bar)
            .child(card_body);
        if let Some(working_snake) = visual.working_snake {
            node_div = node_div.child(render_working_snake_border(
                working_snake,
                vp.zoom,
                node_radius,
            ));
        }
        node_elems.push(node_div.into_any_element());
    }

    if CANVAS_OPEN_RENDER_PENDING.swap(false, Ordering::AcqRel) {
        tracing::info!(
            stage = "workspace_open_first_canvas_render",
            total_nodes,
            visible_nodes,
            element_count = node_elems.len(),
            zoom = vp.zoom,
            window_width = win_w,
            window_height = win_h,
            elapsed_ms = render_start.elapsed().as_secs_f64() * 1000.0,
            "workspace_open_timing"
        );
    }

    div()
        .id("canvas")
        .size_full()
        .relative()
        .on_scroll_wheel(cx.listener(events::on_scroll))
        .on_mouse_down(MouseButton::Left, cx.listener(events::on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(events::on_mouse_down))
        .on_mouse_move(cx.listener(events::on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(events::on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(events::on_mouse_up))
        .children(node_elems)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/canvas_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/chase_canvas_view_contract_tests.rs"]
mod chase_canvas_view_contract_tests;
