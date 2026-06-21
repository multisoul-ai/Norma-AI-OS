//! 画布渲染与交互：pan（空格/中键拖）、zoom（滚轮，绕光标）、节点拖拽、单击选中。
//! viewport culling：屏幕外节点不构建元素。

use crate::app::MultisoulApp;
use crate::cards::card_for;
use crate::cards::chrome::{node_shadow, TITLE_BAR_H};
use canvas::{hit_test, CanvasRect, NodeId, Vec2};
use gpui::{
    div, prelude::*, px, rgba, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, Window,
};
use runtime::{route, InputEvent, InputMode, Route};

#[derive(Clone, Copy)]
pub struct DragState {
    pub node: NodeId,
    pub last_screen: Vec2,
}

#[derive(Default)]
pub struct InteractionState {
    pub space_held: bool,
    pub panning: bool,
    pub last_screen: Option<Vec2>,
    pub drag: Option<DragState>,
}

const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;

fn node_card_radius_px(zoom: f32) -> f32 {
    (16.0 * zoom).clamp(8.0, 22.0)
}

fn node_title_radius_px(zoom: f32) -> f32 {
    node_card_radius_px(zoom)
}

fn node_terminal_radius_px(zoom: f32) -> f32 {
    node_card_radius_px(zoom)
}

fn node_edge_highlight_inset_px(zoom: f32) -> f32 {
    node_card_radius_px(zoom)
}

fn pt_to_vec(p: Point<Pixels>) -> Vec2 {
    Vec2::new(f32::from(p.x), f32::from(p.y))
}

fn is_title_bar_hit(screen_y: f32, node_top_screen_y: f32, zoom: f32) -> bool {
    screen_y <= node_top_screen_y + TITLE_BAR_H * zoom
}

fn click_event_for_node(
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

/// 滚轮 zoom，绕光标保持画布点不动。
fn on_scroll(
    app: &mut MultisoulApp,
    ev: &ScrollWheelEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let _span = tracing::trace_span!("canvas_zoom").entered();
    let cursor = pt_to_vec(ev.position);
    let vp = &mut app.runtime.document.viewport;
    let canvas_pt = vp.screen_to_canvas(cursor);

    let dy = f32::from(ev.delta.pixel_delta(px(1.0)).y); // 归一化滚动量
    let factor = (1.0 + dy * 0.0015).clamp(0.5, 1.5);
    vp.zoom = (vp.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    // 保持光标下画布点不动：pan = cursor - canvas_pt * zoom
    vp.pan = Vec2::new(
        cursor.x - canvas_pt.x * vp.zoom,
        cursor.y - canvas_pt.y * vp.zoom,
    );
    cx.notify();
}

fn on_mouse_down(
    app: &mut MultisoulApp,
    ev: &MouseDownEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);

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
    app.interaction.panning = false;
    app.interaction.drag = None;
    app.interaction.last_screen = None;
    app.motion.clear_transient();
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
        let node = *app.runtime.document.node(id).unwrap();
        let card = card_for(node.kind.node_type());
        let chrome = card.chrome(&node, &app.runtime);
        let (origin, size, selected, title) = (
            node.bounds.origin,
            node.bounds.size,
            node.selected,
            chrome.title,
        );
        let selected = selected || app.runtime.selection.contains(id);
        let canonical_rect = CanvasRect { origin, size };
        let focused = matches!(focus_mode, InputMode::NodeFocused(fid) if fid == id);
        app.motion.ensure_node(id, canonical_rect);
        app.motion.set_selection_focus(id, selected, focused);
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
        let w = visual.rect.size.x * vp.zoom;
        let h = visual.rect.size.y * vp.zoom;

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
            .shadow(node_shadow(visual))
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
                        .child(title),
                )
                .child(
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
                            if let Route::RemoveNode(node_id) =
                                route(&app.runtime.focus.mode(), &event)
                            {
                                if app.remove_node_and_reflow(node_id, window) {
                                    app.interaction.drag = None;
                                    app.interaction.panning = false;
                                    app.interaction.last_screen = None;
                                }
                            }
                            cx.notify();
                        })),
                ),
        );

        // 内容
        node_div = node_div.child(card.body(
            &node,
            &app.runtime,
            theme,
            vp.zoom,
            node_terminal_radius_px(vp.zoom),
            window,
            cx,
        ));

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
#[path = "canvas_view_tests.rs"]
mod tests;
