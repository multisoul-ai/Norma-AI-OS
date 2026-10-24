//! Workspace-wide assistant surfaces. Open drawers reserve space beside the live canvas.

use super::{glass_button_variant, GLASS_TEXT, GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, Stateful, Window};
use runtime::desktop_drawers::{route_drawer, DesktopDrawerEvent, DesktopDrawerSide};
use ui::component::button::{Button, ButtonVariants};

pub(super) const TOP: f32 = 0.0;
pub(super) const BOTTOM: f32 = 0.0;

mod artifacts;
mod brain_toggle;
mod chat;
mod chat_message;
pub(super) mod content;
pub(in crate::workspace::desktop) mod float;
mod ideas_assigned;
mod ideas_keys;
mod ideas_list;
mod ideas_menu;
mod ideas_row;
mod ideas_tabs;
mod layout_save;
mod left_body;
mod left_tabs;
pub(super) mod left_view_slide;
pub(super) mod material;
pub(super) mod motion;
pub(super) mod orb;
mod overview_actions;
mod overview_brain;
mod overview_judging;
mod overview_list;
mod overview_parts;
mod overview_summary;
pub(super) mod resize;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_drawer_surface_tests.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_assistant_conversation_surface_tests.rs"]
mod conversation_surface_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_left_chat_hidden_tests.rs"]
mod left_chat_hidden_tests;

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
#[path = "../../../tests/unit/desktop_orb_motion_tests.rs"]
mod orb_motion_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_drawer_material_tests.rs"]
mod material_tests;

#[cfg(test)]
#[path = "../../../tests/unit/overview_focus_tests.rs"]
mod overview_focus_tests;
#[cfg(test)]
#[path = "../../../tests/unit/desktop_run_overview_shortcut_tests.rs"]
mod run_overview_shortcut_tests;

impl MultisoulApp {
    pub(crate) fn desktop_assistant_layout(&self, window: &Window) -> motion::DesktopDrawerLayout {
        let width = f32::from(window.viewport_size().width);
        if !matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
            && !self.desktop_directory_zoom_active()
        {
            return resize::layout_with_user_widths(
                width,
                0.0,
                0.0,
                &self.desktop_workspace.drawer_widths,
            );
        }
        resize::layout_with_user_widths(
            width,
            f32::from(self.desktop_workspace.drawers.left_open as u8),
            f32::from(self.desktop_workspace.drawers.right_open as u8),
            &self.desktop_workspace.drawer_widths,
        )
    }

    pub(crate) fn render_desktop_assistant_drawers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.reconcile_overview_float_bounds(window, cx);
        self.ensure_workspace_activity_cache(cx);
        self.ensure_drawer_frost(cx);
        let drawers = self.desktop_workspace.drawers;
        self.desktop_workspace
            .drawer_motion
            .retarget(drawers.left_open, drawers.right_open);
        let layout = self.desktop_assistant_layout(window);
        self.desktop_workspace.idea_flight_anchor = ((layout.left_width * 0.5).round() as i32, 240);
        let left_motion = &self.desktop_workspace.drawer_motion.left;
        let left_inset = left_motion.edge_inset(layout.left_width, 0.0, self.motion.reduce_motion);
        let left_opacity = left_motion.opacity(self.motion.reduce_motion);
        let left_visible = left_motion.should_render();
        let right_motion = &self.desktop_workspace.drawer_motion.right;
        let right_inset =
            right_motion.edge_inset(layout.right_width, 0.0, self.motion.reduce_motion);
        let right_opacity = right_motion.opacity(self.motion.reduce_motion);
        let right_visible = right_motion.should_render();
        let left = left_visible.then(|| {
            self.render_assistant_left(layout.left_width, left_inset, left_opacity, window, cx)
        });
        let right = right_visible.then(|| {
            self.render_assistant_right(layout.right_width, right_inset, right_opacity, window, cx)
        });
        let left_handle = drawers.left_open.then(|| {
            self.drawer_resize_handle(DesktopDrawerSide::Left, layout.left_width, left_inset, cx)
        });
        let right_handle = drawers.right_open.then(|| {
            self.drawer_resize_handle(
                DesktopDrawerSide::Right,
                layout.right_width,
                right_inset,
                cx,
            )
        });
        let resize_capture = self.drawer_resize_capture(cx);
        div()
            .id("desktop-assistant-drawers")
            .absolute()
            .inset_0()
            .when(!drawers.left_open, |layer| {
                layer.child(self.drawer_reopen_button("desktop-open-left-drawer", true, cx))
            })
            .when(!drawers.right_open, |layer| {
                layer.child(self.drawer_reopen_button("desktop-open-right-drawer", false, cx))
            })
            .when_some(left, |layer, left| layer.child(left))
            .when_some(right, |layer, right| layer.child(right))
            .when_some(left_handle, |layer, handle| layer.child(handle))
            .when_some(right_handle, |layer, handle| layer.child(handle))
            .when_some(resize_capture, |layer, capture| layer.child(capture))
            .when_some(
                self.render_run_status_focus_sink(window, cx),
                |layer, sink| layer.child(sink),
            )
            .when_some(self.render_floating_overview(window, cx), |layer, panel| {
                layer.child(panel)
            })
            .when_some(self.render_overview_drag_capture(cx), |layer, capture| {
                layer.child(capture)
            })
            .when_some(self.render_idea_badges(window, cx), |layer, badges| {
                layer.child(badges)
            })
            .when_some(self.render_idea_picker(window, cx), |layer, picker| {
                layer.child(picker)
            })
            .when_some(self.render_idea_drag_layer(cx), |layer, capture| {
                layer.child(capture)
            })
    }

    fn drawer_reopen_button(&self, id: &'static str, left: bool, cx: &Context<Self>) -> Div {
        let theme = self.desktop_theme();
        div()
            .absolute()
            .top(px(10.0))
            .when(left, |button| {
                button.left(px(crate::app::BRAND_TOGGLE_LEFT))
            })
            .when(!left, |button| button.right(px(12.0)))
            .child(
                Button::new(id)
                    .debug_selector(move || id.to_string())
                    .w(px(32.0))
                    .h(px(32.0))
                    .p(px(0.0))
                    .child(
                        div().flex().items_center().justify_center().child(
                            svg()
                                .path(if left {
                                    "icons/panel-left-open.svg"
                                } else {
                                    "icons/panel-right-open.svg"
                                })
                                .text_color(rgba(GLASS_TEXT))
                                .w(px(17.0))
                                .h(px(17.0)),
                        ),
                    )
                    .custom(glass_button_variant(theme, cx))
                    .rounded(px(9.0))
                    .tooltip(if left {
                        "展开 Norma 对话"
                    } else {
                        "展开内容浏览器"
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.toggle_desktop_drawer(
                            if left {
                                DesktopDrawerEvent::ToggleLeft
                            } else {
                                DesktopDrawerEvent::ToggleRight
                            },
                            cx,
                        );
                    })),
            )
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

    fn stop_desktop_ego_task(
        &mut self,
        area_id: &str,
        ego_node_id: NodeId,
        expected_agent_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_ref() else {
            return;
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.to_owned()))
            .cloned()
        else {
            return;
        };
        let result = session.update(cx, |project, cx| {
            let state = project
                .runtime
                .ego_task_state(ego_node_id)
                .ok_or("Ego 任务已不存在")?;
            if state.bound_agent_node_id != Some(expected_agent_id)
                || !matches!(
                    state.phase,
                    canvas::EgoCardPhase::PreparingAgent
                        | canvas::EgoCardPhase::AgentWorking
                        | canvas::EgoCardPhase::ResumeRequested
                )
            {
                return Err("Ego 任务状态已变化，请重新查看概览".into());
            }
            project.interrupt_agent_from_desktop_overview(expected_agent_id, cx)
        });
        if let Err(error) = result {
            self.desktop_workspace.error = Some(error.into());
        }
        cx.notify();
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
