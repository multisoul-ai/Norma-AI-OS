//! Mouse adaptation for the pure drawer resize decision in runtime.

use super::motion::{DesktopDrawerLayout, LEFT_MIN_WIDTH, MIN_CANVAS_WIDTH, RIGHT_MIN_WIDTH};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Stateful, Window,
};
use runtime::desktop_drawers::{
    resize_drawer, route_drawer, DesktopDrawerEvent, DesktopDrawerSide,
};

#[derive(Default)]
pub(crate) struct DesktopDrawerWidths {
    pub(crate) left: Option<f32>,
    pub(crate) right: Option<f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct DesktopDrawerResizeSession {
    side: DesktopDrawerSide,
    initial_x: f32,
    initial_width: f32,
}

impl MultisoulApp {
    pub(super) fn drawer_resize_handle(
        &self,
        side: DesktopDrawerSide,
        width: f32,
        inset: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let left = side == DesktopDrawerSide::Left;
        div()
            .id(if left {
                "desktop-left-resize-handle"
            } else {
                "desktop-right-resize-handle"
            })
            .debug_selector(move || {
                if left {
                    "desktop-left-resize-handle"
                } else {
                    "desktop-right-resize-handle"
                }
                .to_string()
            })
            .absolute()
            .top_0()
            .bottom_0()
            .when(left, |handle| handle.left(px(width + inset - 8.0)))
            .when(!left, |handle| handle.right(px(width + inset - 8.0)))
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
                        side,
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
        let layout = self.desktop_assistant_layout(window);
        let (minimum, other_space) = match session.side {
            DesktopDrawerSide::Left => (LEFT_MIN_WIDTH, layout.right_space),
            DesktopDrawerSide::Right => (RIGHT_MIN_WIDTH, layout.left_space),
        };
        let maximum =
            (f32::from(window.viewport_size().width) - other_space - MIN_CANVAS_WIDTH).max(minimum);
        let result = resize_drawer(
            session.side,
            session.initial_width,
            session.initial_x,
            x,
            minimum,
            maximum,
        );
        let event = match session.side {
            DesktopDrawerSide::Left => {
                self.desktop_workspace.drawer_widths.left = Some(result.width);
                DesktopDrawerEvent::ResizeLeft(result.open)
            }
            DesktopDrawerSide::Right => {
                self.desktop_workspace.drawer_widths.right = Some(result.width);
                DesktopDrawerEvent::ResizeRight(result.open)
            }
        };
        let before = self.desktop_workspace.drawers;
        self.desktop_workspace.drawers = route_drawer(before, event);
        if before != self.desktop_workspace.drawers {
            self.desktop_workspace.drawer_motion.retarget(
                self.desktop_workspace.drawers.left_open,
                self.desktop_workspace.drawers.right_open,
            );
            self.wake_animation_frame_loop();
        }
        cx.notify();
    }
}

pub(super) fn layout_with_user_widths(
    window_width: f32,
    left_fraction: f32,
    right_fraction: f32,
    widths: &DesktopDrawerWidths,
) -> DesktopDrawerLayout {
    DesktopDrawerLayout::with_widths(
        window_width,
        left_fraction,
        right_fraction,
        widths.left,
        widths.right,
    )
}
