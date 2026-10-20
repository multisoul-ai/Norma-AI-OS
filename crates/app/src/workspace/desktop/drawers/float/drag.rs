//! 悬浮面板的拖动、限位和焦点落点。

use super::OverviewDrag;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Stateful, Window,
};
use runtime::desktop_drawers::{
    move_overview_offset, overview_key_step, status_display_marks_pin, DesktopDrawerEvent,
    OverviewCanvas,
};
use std::time::Instant;

impl MultisoulApp {
    pub(in crate::workspace::desktop::drawers::float) fn begin_overview_drag(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.click_count >= 2 {
            self.desktop_workspace.overview_drag = None;
            self.toggle_desktop_drawer(DesktopDrawerEvent::ResetOverviewOffset, cx);
            return;
        }
        self.desktop_workspace.overview_float_fade.snap_visible();
        self.desktop_workspace.overview_drag = Some(OverviewDrag {
            origin_x: f32::from(event.position.x),
            origin_y: f32::from(event.position.y),
            origin_offset: self.desktop_workspace.drawers.overview_offset,
            last_notify: None,
        });
        cx.notify();
    }

    pub(in crate::workspace::desktop::drawers::float) fn nudge_overview_from_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        let step = overview_key_step(event.keystroke.modifiers.shift);
        let (dx, dy) = match event.keystroke.key.as_str() {
            "left" => (-step, 0),
            "right" => (step, 0),
            "up" => (0, -step),
            "down" => (0, step),
            _ => return false,
        };
        self.apply_overview_delta(dx as f32, dy as f32, window);
        if let Some(id) = self.desktop_workspace.active_id.clone() {
            self.persist_left_drawer_view(&id, cx);
        }
        cx.notify();
        true
    }

    pub(in crate::workspace::desktop) fn render_overview_drag_capture(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.desktop_workspace.overview_drag.map(|_| {
            div()
                .id("desktop-overview-drag-capture")
                .debug_selector(|| "desktop-overview-drag-capture".to_string())
                .absolute()
                .inset_0()
                .cursor_grabbing()
                .occlude()
                .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, window, cx| {
                    cx.stop_propagation();
                    if event.pressed_button == Some(MouseButton::Left) {
                        app.drag_overview(event, window, cx);
                    } else {
                        app.finish_overview_drag(cx);
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, event: &MouseUpEvent, window, cx| {
                        cx.stop_propagation();
                        app.drag_overview_to(
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            window,
                        );
                        app.finish_overview_drag(cx);
                    }),
                )
        })
    }

    /// 拖动路径只更新偏移并按 16ms 合并重绘，不写 SQLite。
    fn drag_overview(&mut self, event: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        let Some(drag) = self.desktop_workspace.overview_drag else {
            return;
        };
        self.drag_overview_to(
            f32::from(event.position.x),
            f32::from(event.position.y),
            window,
        );
        let now = Instant::now();
        let notify = match drag.last_notify {
            Some(last) => now
                .checked_duration_since(last)
                .map(|elapsed| elapsed >= super::DRAG_NOTIFY_INTERVAL)
                .unwrap_or(true),
            None => true,
        };
        if notify {
            if let Some(drag) = self.desktop_workspace.overview_drag.as_mut() {
                drag.last_notify = Some(now);
            }
            cx.notify();
        }
    }

    fn drag_overview_to(&mut self, x: f32, y: f32, window: &Window) {
        let Some(drag) = self.desktop_workspace.overview_drag else {
            return;
        };
        let canvas = self.overview_canvas(window);
        self.desktop_workspace.drawers.overview_offset = move_overview_offset(
            drag.origin_offset,
            x - drag.origin_x,
            y - drag.origin_y,
            canvas,
        );
    }

    fn finish_overview_drag(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.overview_drag.take().is_none() {
            return;
        }
        if let Some(id) = self.desktop_workspace.active_id.clone() {
            self.persist_left_drawer_view(&id, cx);
        }
        cx.notify();
    }

    fn apply_overview_delta(&mut self, dx: f32, dy: f32, window: &Window) {
        let canvas = self.overview_canvas(window);
        let start = self.desktop_workspace.drawers.overview_offset;
        self.desktop_workspace.drawers.overview_offset =
            move_overview_offset(start, dx, dy, canvas);
    }

    pub(in crate::workspace::desktop) fn reconcile_overview_float_bounds(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.desktop_workspace.drawers.overview_floating
            || self.desktop_workspace.overview_drag.is_some()
        {
            return;
        }
        let canvas = self.overview_canvas(window);
        let clamped = runtime::desktop_drawers::clamp_overview_offset(
            self.desktop_workspace.drawers.overview_offset,
            canvas,
        );
        if clamped == self.desktop_workspace.drawers.overview_offset {
            return;
        }
        self.desktop_workspace.drawers.overview_offset = clamped;
        let Some(id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        // 渲染里不能直接 spawn 写库。等本帧效果结束后再异步保存。
        cx.defer_in(window, move |app, _window, cx| {
            app.persist_left_drawer_view(&id, cx);
        });
    }

    pub(in crate::workspace::desktop::drawers::float) fn overview_canvas(
        &self,
        window: &Window,
    ) -> OverviewCanvas {
        let layout = self.desktop_assistant_layout(window);
        OverviewCanvas {
            width: layout.canvas_width,
            height: f32::from(window.viewport_size().height),
        }
    }

    pub(in crate::workspace::desktop) fn overview_float_holds_focus(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> bool {
        if !self.desktop_workspace.drawers.overview_floating {
            return false;
        }
        self.desktop_workspace
            .overview_float_focus
            .as_ref()
            .is_some_and(|handle| handle.contains_focused(window, cx))
            || self
                .desktop_workspace
                .overview_move_focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
    }

    pub(in crate::workspace::desktop) fn focus_run_status(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_status_focus(cx).focus(window);
    }

    pub(in crate::workspace::desktop) fn render_run_status_focus_sink(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !status_display_marks_pin(&self.desktop_workspace.drawers)
            || self.status_capsule_visible()
        {
            return None;
        }
        let layout = self.desktop_assistant_layout(window);
        let focus = self.run_status_focus(cx);
        // #126 的状态胶囊接过这个焦点。胶囊还不存在时，这里只提供可聚焦的落点。
        Some(
            div()
                .id("desktop-run-status")
                .debug_selector(|| "desktop-run-status".to_string())
                .absolute()
                .top(px(10.0))
                .left(px(layout.left_space + layout.canvas_width - 20.0))
                .w(px(8.0))
                .h(px(8.0))
                .tab_index(0)
                .track_focus(&focus),
        )
    }

    pub(in crate::workspace::desktop::drawers::float) fn overview_float_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::FocusHandle {
        self.desktop_workspace
            .overview_float_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    pub(in crate::workspace::desktop::drawers::float) fn overview_move_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::FocusHandle {
        self.desktop_workspace
            .overview_move_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    pub(in crate::workspace::desktop) fn run_status_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::FocusHandle {
        self.desktop_workspace
            .run_status_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }
}
