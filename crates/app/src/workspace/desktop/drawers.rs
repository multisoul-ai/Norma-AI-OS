//! Workspace-wide assistant surfaces. Open drawers reserve space beside the live canvas.

use super::glass_button_variant;
use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, Stateful, Window};
use runtime::desktop_drawers::{route_drawer, DesktopDrawerEvent, DesktopDrawerState};
use ui::component::button::{Button, ButtonVariants};

pub(super) const TOP: f32 = 0.0;
pub(super) const BOTTOM: f32 = 0.0;

mod artifacts;
pub(super) mod content;
pub(super) mod material;
pub(super) mod motion;
pub(super) mod overview_questions;
pub(super) mod resize;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_new_card_reflow_tests.rs"]
mod new_card_reflow_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_card_layout_adapt_tests.rs"]
mod card_layout_adapt_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_cmd_d_delete_tests.rs"]
mod cmd_d_delete_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_drawer_material_tests.rs"]
mod material_tests;

impl MultisoulApp {
    pub(crate) fn desktop_assistant_layout(&self, window: &Window) -> motion::DesktopDrawerLayout {
        let width = f32::from(window.viewport_size().width);
        if !matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
            && !self.desktop_directory_zoom_active()
        {
            return resize::layout_with_user_width(
                width,
                0.0,
                &self.desktop_workspace.drawer_widths,
            );
        }
        if self.staff.enabled && width < 1180.0 {
            return motion::DesktopDrawerLayout::with_width(
                width,
                0.0,
                self.desktop_workspace.drawer_widths.right,
            );
        }
        if self.staff.enabled {
            // 任务栏之外仍保留既有最小320px内容区；只夹当前布局，不改画布宽度偏好。
            let maximum =
                (width - crate::app::staff::sidebar_width(window) - motion::MIN_CANVAS_WIDTH)
                    .max(motion::RIGHT_MIN_WIDTH);
            let preferred = self.desktop_workspace.drawer_widths.right.unwrap_or(380.0);
            return motion::DesktopDrawerLayout::with_width(
                width,
                f32::from(self.desktop_workspace.drawers.right_open as u8),
                Some(preferred.min(maximum)),
            );
        }
        resize::layout_with_user_width(
            width,
            f32::from(self.desktop_workspace.drawers.right_open as u8),
            &self.desktop_workspace.drawer_widths,
        )
    }

    pub(crate) fn render_desktop_assistant_drawers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // 原型的窄窗断点隐藏内容栏；保留展开状态和宽度，回到宽窗自动恢复。
        if self.staff.enabled && f32::from(window.viewport_size().width) < 1180.0 {
            return div().id("desktop-assistant-drawers");
        }
        self.ensure_workspace_activity_cache(cx);
        self.ensure_drawer_frost(cx);
        let drawers = self.desktop_workspace.drawers;
        self.desktop_workspace
            .drawer_motion
            .retarget(drawers.right_open);
        let layout = self.desktop_assistant_layout(window);
        let right_motion = &self.desktop_workspace.drawer_motion.right;
        let right_inset =
            right_motion.edge_inset(layout.right_width, 0.0, self.motion.reduce_motion);
        let right_opacity = right_motion.opacity(self.motion.reduce_motion);
        let right_visible = right_motion.should_render();
        let right = right_visible.then(|| {
            self.render_assistant_right(layout.right_width, right_inset, right_opacity, window, cx)
        });
        let right_handle = drawers
            .right_open
            .then(|| self.drawer_resize_handle(layout.right_width, right_inset, cx));
        let resize_capture = self.drawer_resize_capture(cx);
        div()
            .id("desktop-assistant-drawers")
            .absolute()
            .inset_0()
            .when(!drawers.right_open, |layer| {
                layer.child(self.drawer_reopen_button(cx))
            })
            .when_some(right, |layer, right| layer.child(right))
            .when_some(right_handle, |layer, handle| layer.child(handle))
            .when_some(resize_capture, |layer, capture| layer.child(capture))
    }

    fn drawer_reopen_button(&self, cx: &Context<Self>) -> Div {
        let theme = self.desktop_theme();
        div().absolute().top(px(10.0)).right(px(12.0)).child(
            Button::new("desktop-open-right-drawer")
                .debug_selector(|| "desktop-open-right-drawer".to_string())
                .w(px(32.0))
                .h(px(32.0))
                .p(px(0.0))
                .child(
                    div().flex().items_center().justify_center().child(
                        svg()
                            .path("icons/panel-right-open.svg")
                            .text_color(rgba(theme.text_primary))
                            .w(px(17.0))
                            .h(px(17.0)),
                    ),
                )
                .custom(glass_button_variant(theme, cx))
                .rounded(px(9.0))
                .tooltip(ui::i18n::text("drawers.expand_content"))
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
                })),
        )
    }

    pub(crate) fn apply_desktop_drawers(
        &mut self,
        next: DesktopDrawerState,
        cx: &mut Context<Self>,
    ) {
        let before = self.desktop_workspace.drawers;
        if before.right_open && !next.right_open {
            self.begin_desktop_content_navigation();
        }
        self.desktop_workspace.drawers = next;
        self.desktop_workspace
            .drawer_motion
            .retarget(next.right_open);
        if !before.right_open && next.right_open {
            self.load_selected_desktop_content(cx);
        }
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn activate_desktop_workspace_view(&mut self, id: &str, _cx: &mut Context<Self>) {
        self.desktop_workspace.active_id = Some(id.to_string());
    }

    pub(crate) fn dismiss_desktop_switcher_on_escape(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key != "escape"
            || event.keystroke.modifiers.modified()
            || !self.desktop_workspace.switcher_open
        {
            return false;
        }
        self.dismiss_desktop_switcher(window, cx);
        cx.stop_propagation();
        true
    }

    fn toggle_desktop_drawer(&mut self, event: DesktopDrawerEvent, cx: &mut Context<Self>) {
        let next = route_drawer(self.desktop_workspace.drawers, event);
        self.apply_desktop_drawers(next, cx);
    }

    pub(crate) fn locate_desktop_agent(
        &mut self,
        area_id: &str,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id.to_owned()))
            .cloned()
        else {
            return;
        };
        if session.read(cx).runtime.document.node(node_id).is_none() {
            return;
        }
        self.expand_desktop_project_area(area_id.to_owned(), window, cx);
        session.update(cx, |project, cx| {
            project.runtime.selection.select_only(node_id);
            project.runtime.focus.focus_node(node_id);
            cx.notify();
        });
    }
}

pub(super) fn content_node_id(id: &str) -> NodeId {
    let prefix = id
        .chars()
        .filter(|ch| *ch != '-')
        .take(15)
        .collect::<String>();
    u64::from_str_radix(&prefix, 16).unwrap_or(0) | (1_u64 << 63)
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_reader_chrome_surface_tests.rs"]
mod reader_chrome_surface_tests;
