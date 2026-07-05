//! 画布鼠标与滚轮事件处理：pan、节点拖拽、单击选中、文字选择。

use crate::app::MultisoulApp;
use crate::canvas::geometry::{
    apply_canvas_wheel_scroll, click_event_for_node, is_title_bar_hit, pt_to_vec,
};
use crate::canvas::interaction::DragState;
use crate::canvas::scroll::{
    terminal_scroll_lines_from_pixel_delta, wheel_scroll_owner_for_point, WheelScrollOwner,
};
use crate::canvas::terminal_hit::terminal_text_point_for_screen;
use canvas::{hit_test, Vec2};
use gpui::{
    px, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent,
    Window,
};
use runtime::{route, NodeScroll, Route};
use std::time::{Duration, Instant};

const MOTION_NOTIFY_INTERVAL: Duration = Duration::from_millis(16);

/// 滚轮/触控板平移画布；卡片内部滚动优先于画布滚动。
pub(super) fn on_scroll(
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

pub(super) fn on_mouse_down(
    app: &mut MultisoulApp,
    ev: &MouseDownEvent,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);
    if should_focus_app_root_for_mouse_down(ev.button) {
        app.focus_handle.focus(window);
    }
    // 画布上任意鼠标按下都应关闭展开的 ✚ 菜单（避免菜单悬浮在画布之上）。
    // 不 return：让正常画布点击逻辑继续执行。
    if app.interaction.new_menu_open {
        app.interaction.close_new_menu();
        cx.notify();
    }
    app.interaction.keyboard_selected_node = None;

    // 中键 或 空格+左键 → 开始 pan。
    let want_pan = ev.button == MouseButton::Middle
        || (ev.button == MouseButton::Left && app.interaction.space_held);
    if want_pan {
        app.interaction.panning = true;
        app.interaction.last_screen = Some(screen);
        app.interaction.last_motion_notify = None;
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
                app.mark_node_viewed_if_needed(id);
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
            app.mark_node_viewed_if_needed(id);
            app.motion.set_pressed(id, true);
            // 命中标题栏 → 开始拖拽。
            if is_title_bar {
                app.interaction.drag = Some(DragState {
                    node: id,
                    last_screen: screen,
                });
                app.interaction.last_motion_notify = None;
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
            app.mark_node_viewed_if_needed(id);
            app.motion.set_pressed(id, true);
        }
        _ => {}
    }
    cx.notify();
}

pub(super) fn should_focus_app_root_for_mouse_down(button: MouseButton) -> bool {
    matches!(button, MouseButton::Left | MouseButton::Middle)
}

pub(super) fn should_notify_motion_frame(last: Option<Instant>, now: Instant) -> bool {
    match last {
        Some(last) => now
            .checked_duration_since(last)
            .map(|elapsed| elapsed >= MOTION_NOTIFY_INTERVAL)
            .unwrap_or(true),
        None => true,
    }
}

pub(super) fn on_mouse_move(
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
        let now = Instant::now();
        if should_notify_motion_frame(app.interaction.last_motion_notify, now) {
            app.interaction.last_motion_notify = Some(now);
            cx.notify();
        }
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
        let now = Instant::now();
        if should_notify_motion_frame(app.interaction.last_motion_notify, now) {
            app.interaction.last_motion_notify = Some(now);
            cx.notify();
        }
    }
}

pub(super) fn on_mouse_up(
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
    app.interaction.last_motion_notify = None;
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
