//! Workspace-wide assistant surfaces. Open drawers reserve space beside the live canvas.

use super::{glass_button_variant, GLASS_TEXT, GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, ElementId, Stateful, Window};
use runtime::desktop_drawers::{route_drawer, DesktopDrawerEvent, DesktopDrawerSide};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, StyledExt,
};

pub(super) const TOP: f32 = 0.0;
pub(super) const BOTTOM: f32 = 0.0;

mod artifacts;
mod chat;
mod chat_message;
mod content;
pub(super) mod material;
pub(super) mod motion;
pub(super) mod orb;
pub(super) mod resize;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_drawer_surface_tests.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_assistant_conversation_surface_tests.rs"]
mod conversation_surface_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_new_card_reflow_tests.rs"]
mod new_card_reflow_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_orb_motion_tests.rs"]
mod orb_motion_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_drawer_material_tests.rs"]
mod material_tests;

impl MultisoulApp {
    pub(super) fn hide_left_drawer_for_project(&mut self) {
        if self.desktop_workspace.left_open_before_project.is_some() {
            return;
        }
        let previous = self.desktop_workspace.drawers.left_open;
        self.desktop_workspace.left_open_before_project = Some(previous);
        self.desktop_workspace.drawers = route_drawer(
            self.desktop_workspace.drawers,
            DesktopDrawerEvent::ResizeLeft(false),
        );
        self.desktop_workspace.drawer_motion.settle_left(false);
        self.desktop_workspace.drawer_resize = None;
    }

    pub(super) fn restore_left_drawer_after_project(&mut self) {
        let Some(previous) = self.desktop_workspace.left_open_before_project.take() else {
            return;
        };
        self.desktop_workspace.drawers = route_drawer(
            self.desktop_workspace.drawers,
            DesktopDrawerEvent::ResizeLeft(previous),
        );
        self.desktop_workspace.drawer_motion.settle_left(previous);
    }

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
        self.ensure_drawer_frost(cx);
        let drawers = self.desktop_workspace.drawers;
        self.desktop_workspace
            .drawer_motion
            .retarget(drawers.left_open, drawers.right_open);
        let layout = self.desktop_assistant_layout(window);
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
        let overview = left_visible.then(|| {
            self.render_floating_agent_overview(layout.left_width, left_inset, left_opacity, cx)
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
            .when_some(overview, |layer, overview| layer.child(overview))
            .when_some(left_handle, |layer, handle| layer.child(handle))
            .when_some(right_handle, |layer, handle| layer.child(handle))
            .when_some(resize_capture, |layer, capture| layer.child(capture))
    }

    fn drawer_reopen_button(&self, id: &'static str, left: bool, cx: &Context<Self>) -> Div {
        let theme = self.desktop_theme();
        div()
            .absolute()
            .top(px(10.0))
            .when(left, |button| button.left(px(82.0)))
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

    fn render_agent_overview(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let workspace_id = self
            .desktop_workspace
            .active_id
            .as_deref()
            .unwrap_or_default();
        div()
            .id("desktop-agent-overview-list")
            .debug_selector(|| "desktop-agent-overview-list".to_string())
            .max_h(px(236.0))
            .overflow_y_scroll()
            .px(px(10.0))
            .pb(px(10.0))
            .children(self.desktop_workspace.areas.iter().map(|area| {
                let rows = self.desktop_agent_rows(workspace_id, &area.id, cx);
                div()
                    .id(ElementId::Name(
                        format!("desktop-agent-project-{}", area.id).into(),
                    ))
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_semibold()
                            .text_color(rgba(GLASS_TEXT_SECONDARY))
                            .child(area.display_name.clone()),
                    )
                    .when(rows.is_empty(), |group| {
                        group.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgba(GLASS_TEXT_MUTED))
                                .child("尚无已加载的智能体"),
                        )
                    })
                    .children(rows.into_iter().map(|row| {
                        let area_id = row.area_id.clone();
                        let node_id = row.node_id;
                        let needs_approval = row.needs_approval;
                        let can_locate_approval = row.can_locate_approval;
                        let is_ego = row.is_ego;
                        let ego_stop_agent = row.ego_stop_agent;
                        let run_id = row.run_id.clone();
                        let turn_id = row.turn_id.clone();
                        let button_id = format!("desktop-agent-jump-{area_id}-{node_id}");
                        Button::new(ElementId::Name(button_id.clone().into()))
                            .disabled(
                                row.stale
                                    || (needs_approval && !can_locate_approval)
                                    || (is_ego && ego_stop_agent.is_none()),
                            )
                            .debug_selector(move || button_id.clone())
                            .w_full()
                            .h(px(54.0))
                            .p(px(8.0))
                            .rounded(px(8.0))
                            .custom(glass_button_variant(theme, cx))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(format!("{} · {}", row.label, row.state))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgba(if row.needs_approval {
                                                theme.accent_cyan
                                            } else {
                                                GLASS_TEXT_SECONDARY
                                            }))
                                            .child(row.task),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgba(theme.accent_cyan))
                                    .child(if row.stale {
                                        "项目尚未加载"
                                    } else if is_ego && ego_stop_agent.is_some() {
                                        "停止任务 →"
                                    } else if is_ego {
                                        "暂无可停止的任务"
                                    } else if needs_approval && !can_locate_approval {
                                        "审批信息待同步"
                                    } else if needs_approval {
                                        "处理审批 →"
                                    } else {
                                        "定位卡片 →"
                                    }),
                            )
                            .on_click(cx.listener(move |app, _, window, cx| {
                                if let Some(agent_id) = ego_stop_agent {
                                    app.stop_desktop_ego_task(&area_id, node_id, agent_id, cx);
                                } else if can_locate_approval {
                                    app.locate_desktop_approval(
                                        &area_id,
                                        node_id,
                                        run_id.as_deref(),
                                        turn_id.as_deref(),
                                        window,
                                        cx,
                                    );
                                } else {
                                    app.locate_desktop_agent(&area_id, node_id, window, cx);
                                }
                            }))
                    }))
            }))
            .child(self.render_desktop_artifacts(cx))
    }

    fn toggle_desktop_drawer(&mut self, event: DesktopDrawerEvent, cx: &mut Context<Self>) {
        let before = self.desktop_workspace.drawers;
        self.desktop_workspace.drawers = route_drawer(before, event);
        self.desktop_workspace.drawer_motion.retarget(
            self.desktop_workspace.drawers.left_open,
            self.desktop_workspace.drawers.right_open,
        );
        if !before.right_open && self.desktop_workspace.drawers.right_open {
            self.load_selected_desktop_content(cx);
        }
        self.wake_animation_frame_loop();
        cx.notify();
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

pub(super) fn is_image_file(path: &str) -> bool {
    [
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "svg",
    ]
    .iter()
    .any(|extension| {
        path.to_ascii_lowercase()
            .ends_with(&format!(".{extension}"))
    })
}

pub(super) fn is_web_file(path: &str) -> bool {
    ["html", "htm", "pdf", "mp4", "mov", "webm"]
        .iter()
        .any(|extension| {
            path.to_ascii_lowercase()
                .ends_with(&format!(".{extension}"))
        })
}
