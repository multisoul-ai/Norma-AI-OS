//! Mouse adaptation for the pure drawer resize decision in runtime.

use super::motion::{DesktopDrawerLayout, MIN_CANVAS_WIDTH, RIGHT_MIN_WIDTH};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Stateful, Window,
};
use runtime::desktop_drawers::{resize_drawer, route_drawer, DesktopDrawerEvent};

#[derive(Default)]
pub(crate) struct DesktopDrawerWidths {
    pub(crate) right: Option<f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct DesktopDrawerResizeSession {
    initial_x: f32,
    initial_width: f32,
}

impl MultisoulApp {
    pub(super) fn drawer_resize_handle(
        &self,
        width: f32,
        inset: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("desktop-right-resize-handle")
            .debug_selector(|| "desktop-right-resize-handle".to_string())
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(width + inset - 8.0))
            .w(px(16.0))
            .cursor_ew_resize()
            .occlude()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(7.5))
                    .w(px(1.0))
                    .bg(rgba(0xffffff25)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    app.desktop_workspace.drawer_resize = Some(DesktopDrawerResizeSession {
                        initial_x: f32::from(event.position.x),
                        initial_width: width,
                    });
                    cx.notify();
                }),
            )
    }

    pub(super) fn drawer_resize_capture(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        self.desktop_workspace.drawer_resize.map(|_| {
            div()
                .id("desktop-drawer-resize-capture")
                .debug_selector(|| "desktop-drawer-resize-capture".to_string())
                .absolute()
                .inset_0()
                .cursor_ew_resize()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, window, cx| {
                    cx.stop_propagation();
                    if event.pressed_button == Some(MouseButton::Left) {
                        app.update_desktop_drawer_resize(f32::from(event.position.x), window, cx);
                    } else {
                        app.desktop_workspace.drawer_resize = None;
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, event: &MouseUpEvent, window, cx| {
                        cx.stop_propagation();
                        app.update_desktop_drawer_resize(f32::from(event.position.x), window, cx);
                        app.desktop_workspace.drawer_resize = None;
                        cx.notify();
                    }),
                )
        })
    }

    fn update_desktop_drawer_resize(&mut self, x: f32, window: &Window, cx: &mut Context<Self>) {
        let Some(session) = self.desktop_workspace.drawer_resize else {
            return;
        };
        let maximum =
            (f32::from(window.viewport_size().width) - MIN_CANVAS_WIDTH).max(RIGHT_MIN_WIDTH);
        let result = resize_drawer(
            session.initial_width,
            session.initial_x,
            x,
            RIGHT_MIN_WIDTH,
            maximum,
        );
        self.desktop_workspace.drawer_widths.right = Some(result.width);
        let before = self.desktop_workspace.drawers;
        let next = route_drawer(before, DesktopDrawerEvent::ResizeRight(result.open));
        if before != next {
            self.apply_desktop_drawers(next, cx);
        }
        cx.notify();
    }
}

pub(super) fn layout_with_user_width(
    window_width: f32,
    right_fraction: f32,
    widths: &DesktopDrawerWidths,
) -> DesktopDrawerLayout {
    DesktopDrawerLayout::with_width(window_width, right_fraction, widths.right)
}
