//! 画布渲染与交互：pan（滚轮、空格/中键拖）、节点拖拽、单击选中。
//! viewport culling：屏幕外节点不构建元素。

use crate::app::MultisoulApp;
use crate::canvas::geometry::{
    apply_canvas_wheel_scroll, click_event_for_node, is_title_bar_hit, node_card_radius_px,
    node_edge_highlight_inset_px, node_terminal_radius_px, node_title_radius_px, pt_to_vec,
    terminal_content_local_position,
};
use crate::canvas::interaction::DragState;
#[cfg(test)]
use crate::canvas::interaction::InteractionState;
use crate::canvas::scroll::{
    terminal_scroll_lines_from_pixel_delta, wheel_scroll_owner_for_point, WheelScrollOwner,
};
use crate::canvas::text_selection::TextPoint;
use crate::cards::agent::card::{agent_breathing_intent, agent_chase_intent};
use crate::cards::chrome::{
    node_shadow, render_task_badge, with_chase_overlays, CORNER_GLOW_MIN_SCREEN_PX, TITLE_BAR_H,
};
use crate::cards::term::selection::terminal_point_for_local_position;
use crate::cards::term::view::terminal_render_metrics;
use crate::cards::{card_for, CardBodyContext};
use canvas::{hit_test, CanvasRect, NodeId, NodeType, Vec2};
use gpui::{
    div, prelude::*, px, rgba, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollWheelEvent, Window,
};
use runtime::{route, InputEvent, InputMode, NodeScroll, NodeView, Route};

fn terminal_text_point_for_screen(
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

/// 滚轮/触控板平移画布；卡片内部滚动优先于画布滚动。
fn on_scroll(
    app: &mut MultisoulApp,
    ev: &ScrollWheelEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let _span = tracing::trace_span!("canvas_scroll").entered();
    let cursor = pt_to_vec(ev.position);
    let owner = wheel_scroll_owner_for_point(
        app.runtime.document.nodes(),
        &app.runtime.selection,
        cursor,
        app.runtime.document.viewport,
    );
    match owner {
        WheelScrollOwner::NativeElement(_) => {
            cx.stop_propagation();
            return;
        }
        WheelScrollOwner::TerminalScrollback(id) => {
            cx.stop_propagation();
            let dy = f32::from(ev.delta.pixel_delta(px(1.0)).y);
            let lines = terminal_scroll_lines_from_pixel_delta(dy);
            if lines != 0 {
                match app.runtime.scroll_node(id, NodeScroll::TermLines(lines)) {
                    Ok(true) => cx.notify(),
                    Ok(false) => {}
                    Err(error) => tracing::debug!(?id, %error, "failed to scroll terminal card"),
                }
            }
            return;
        }
        WheelScrollOwner::CanvasViewport => {}
    }

    cx.stop_propagation();
    let delta = ev.delta.pixel_delta(px(1.0));
    let pixel_delta = Vec2::new(f32::from(delta.x), f32::from(delta.y));
    if apply_canvas_wheel_scroll(
        &mut app.runtime.document.viewport,
        pixel_delta,
        app.interaction.canvas_scroll_enabled,
    ) {
        app.mark_canvas_dirty();
        cx.notify();
    }
}

fn on_mouse_down(
    app: &mut MultisoulApp,
    ev: &MouseDownEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);
    app.interaction.keyboard_selected_node = None;

    // 中键 或 空格+左键 → 开始 pan。
    let want_pan = ev.button == MouseButton::Middle
        || (ev.button == MouseButton::Left && app.interaction.space_held);
    if want_pan {
        app.interaction.panning = true;
        app.interaction.last_screen = Some(screen);
        return;
    }

    if ev.button != MouseButton::Left {
        return;
    }

    let canvas_pt = app.runtime.document.viewport.screen_to_canvas(screen);
    let hit = hit_test(app.runtime.document.nodes(), canvas_pt);

    let (is_focusable, is_title_bar) = if let Some(id) = hit {
        if let Some(node) = app.runtime.document.node(id) {
            let top_screen = app
                .runtime
                .document
                .viewport
                .canvas_to_screen(node.bounds.origin);
            (
                app.runtime.node_caps(id).focusable,
                is_title_bar_hit(screen.y, top_screen.y, app.runtime.document.viewport.zoom),
            )
        } else {
            (false, false)
        }
    } else {
        (false, false)
    };
    let event = click_event_for_node(hit, is_focusable, is_title_bar, ev.click_count);

    if let Some(id) = hit {
        if !is_title_bar {
            if let Some(point) = terminal_text_point_for_screen(app, id, screen) {
                app.runtime.focus.focus_node(id);
                app.runtime.selection.select_only(id);
                app.motion.set_pressed(id, true);
                app.interaction.begin_text_selection(id, point);
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        app.interaction.clear_text_selection();
    } else {
        app.interaction.clear_text_selection();
    }

    match route(&app.runtime.focus.mode(), &event) {
        Route::SelectNode(id) => {
            app.runtime.selection.select_only(id);
            app.runtime.focus.exit_to_canvas();
            app.motion.set_pressed(id, true);
            // 命中标题栏 → 开始拖拽。
            if is_title_bar {
                app.interaction.drag = Some(DragState {
                    node: id,
                    last_screen: screen,
                });
                app.motion.set_dragging(id, true);
            }
        }
        Route::ClearSelection => {
            app.runtime.selection.clear();
            app.runtime.focus.exit_to_canvas();
        }
        Route::FocusNode(id) => {
            app.runtime.focus.focus_node(id);
            app.runtime.selection.select_only(id);
            app.motion.set_pressed(id, true);
        }
        _ => {}
    }
    cx.notify();
}

fn on_mouse_move(
    app: &mut MultisoulApp,
    ev: &MouseMoveEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);

    if let Some(selection) = app.interaction.active_text_selection {
        if selection.dragging {
            if let Some(point) = terminal_text_point_for_screen(app, selection.node_id, screen) {
                if app
                    .interaction
                    .update_text_selection_head(selection.node_id, point)
                {
                    cx.notify();
                }
            } else {
                app.interaction.clear_text_selection();
                cx.notify();
            }
            return;
        }
    }

    if app.interaction.panning {
        if let Some(last) = app.interaction.last_screen {
            let delta = screen - last;
            let vp = &mut app.runtime.document.viewport;
            vp.pan = vp.pan + delta; // pan 直接按屏幕位移走
        }
        app.interaction.last_screen = Some(screen);
        cx.notify();
        return;
    }

    if let Some(drag) = app.interaction.drag {
        let zoom = app.runtime.document.viewport.zoom;
        let screen_delta = screen - drag.last_screen;
        // 屏幕位移 → 画布位移（除以 zoom）。
        let canvas_delta = Vec2::new(screen_delta.x / zoom, screen_delta.y / zoom);
        app.runtime.document.move_node(drag.node, canvas_delta);
        if let Some(node) = app.runtime.document.node(drag.node) {
            app.motion.sync_drag_rect(drag.node, node.bounds);
        }
        app.interaction.drag = Some(DragState {
            node: drag.node,
            last_screen: screen,
        });
        cx.notify();
    }
}

fn on_mouse_up(
    app: &mut MultisoulApp,
    _ev: &MouseUpEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let should_persist_canvas = app.interaction.panning || app.interaction.drag.is_some();
    let was_text_selecting = app
        .interaction
        .active_text_selection
        .map(|selection| selection.dragging)
        .unwrap_or(false);
    app.interaction.panning = false;
    app.interaction.drag = None;
    app.interaction.last_screen = None;
    app.motion.clear_transient();
    if was_text_selecting {
        app.interaction.finish_text_selection();
        cx.notify();
        return;
    }
    if should_persist_canvas {
        app.mark_canvas_dirty();
    }
    cx.notify();
}

pub fn render_canvas(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let _span =
        tracing::trace_span!("render_canvas", nodes = app.runtime.document.nodes().len()).entered();

    let theme = app.theme;
    let vp = app.runtime.document.viewport;
    let win = window.viewport_size();
    let (win_w, win_h) = (f32::from(win.width), f32::from(win.height));
    let focus_mode = app.runtime.focus.mode();

    // 先收集需要渲染的节点数据（避免借用冲突）。
    let mut node_elems = Vec::new();
    let node_ids: Vec<NodeId> = app.runtime.document.nodes().iter().map(|n| n.id).collect();
    app.motion.retain_nodes(node_ids.iter().copied());
    for id in node_ids {
        let node = app.runtime.document.node(id).unwrap().clone();
        let card = card_for(node.kind.node_type());
        let chrome = card.chrome(&node, &app.runtime);
        let (origin, size, selected, title, task_badge) = (
            node.bounds.origin,
            node.bounds.size,
            node.selected,
            chrome.title,
            chrome.task_badge,
        );
        let refreshable = chrome.refreshable;
        let selected = selected || app.runtime.selection.contains(id);
        let canonical_rect = CanvasRect { origin, size };
        let focused = matches!(focus_mode, InputMode::NodeFocused(fid) if fid == id);
        app.motion.ensure_node(id, canonical_rect);
        app.motion.set_selection_focus(id, selected, focused);
        let is_agent = node.kind.node_type() == NodeType::Agent;
        let agent_state = is_agent.then(|| app.runtime.agent_task_state(id)).flatten();
        let breathing_intent = agent_breathing_intent(agent_state.clone());
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
            .border_color(rgba(stroke))
            .shadow(node_shadow(
                visual,
                vp.zoom,
                w >= CORNER_GLOW_MIN_SCREEN_PX && h >= CORNER_GLOW_MIN_SCREEN_PX,
            ))
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

        node_div = node_div.child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(node_edge_highlight_inset_px(vp.zoom)))
                .right(px(node_edge_highlight_inset_px(vp.zoom)))
                .h(px(1.0))
                .bg(rgba(visual.edge_highlight_color)),
        );

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
                    let event = InputEvent::CloseNode { node: id };
                    if let Route::RemoveNode(node_id) = route(&app.runtime.focus.mode(), &event) {
                        if app.remove_node_and_reflow(node_id, window) {
                            app.interaction.drag = None;
                            app.interaction.panning = false;
                            app.interaction.last_screen = None;
                        }
                    }
                    cx.notify();
                })),
        );

        node_div = node_div.child(
            div()
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
                        .gap_2()
                        .child(
                            div()
                                .w(px(7.0 * vp.zoom))
                                .h(px(7.0 * vp.zoom))
                                .rounded(px(4.0 * vp.zoom))
                                .bg(rgba(status)),
                        )
                        .child(title)
                        .when_some(task_badge, |this, badge| {
                            this.child(render_task_badge(badge, vp.zoom, theme))
                        }),
                )
                .child(title_actions),
        );

        // 内容
        node_div = node_div.child(card.body(
            &node,
            CardBodyContext {
                runtime: &app.runtime,
                preview_state: app.previews.state_for(id),
                text_selection: app.interaction.active_text_selection,
                theme,
                zoom: vp.zoom,
                bottom_radius_px: node_terminal_radius_px(vp.zoom),
            },
        ));
        if let Some(chase) = visual.chase_glow {
            node_div = with_chase_overlays(node_div, chase, w, h, vp.zoom);
        }
        node_elems.push(node_div);
    }

    div()
        .id("canvas")
        .size_full()
        .relative()
        .on_scroll_wheel(cx.listener(on_scroll))
        .on_mouse_down(MouseButton::Left, cx.listener(on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(on_mouse_down))
        .on_mouse_move(cx.listener(on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(on_mouse_up))
        .children(node_elems)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/canvas_view_tests.rs"]
mod tests;
