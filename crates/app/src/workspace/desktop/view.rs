use super::GLASS_TEXT_SECONDARY;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};

impl MultisoulApp {
    pub(crate) fn render_desktop_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.sync_desktop_project_layout(window, cx);
        self.sync_desktop_input_theme(
            self.desktop_workspace.expanded_area_id.is_some()
                && !self.desktop_workspace.switcher_open,
            cx,
        );
        if self.desktop_workspace.expanded_area_id.is_some()
            && self
                .desktop_workspace
                .transition
                .as_ref()
                .is_some_and(|t| t.projects.is_some())
        {
            let world = div()
                .id("desktop-workspace-world")
                .absolute()
                .inset_0()
                .child(self.render_workspace_backdrop())
                .child(self.render_desktop_project_switch(cx));
            return self.clip_desktop_directory_workspace(world, window);
        }
        let theme = self.desktop_theme();
        #[cfg(target_os = "macos")]
        self.sync_canvas_video_wallpaper(cx);
        let active_workspace = self.desktop_workspace.active_id.as_deref();
        let workspace_frame = self
            .desktop_workspace
            .transition
            .as_ref()
            .and_then(|transition| match &transition.subject {
                super::camera::DesktopCanvasSubject::Workspace(workspace_id)
                    if Some(workspace_id.as_str()) == active_workspace =>
                {
                    Some(transition.rect())
                }
                _ => None,
            });
        let transition_project =
            self.desktop_workspace
                .transition
                .as_ref()
                .and_then(|transition| match &transition.subject {
                    super::camera::DesktopCanvasSubject::Project {
                        workspace_id,
                        area_id,
                    } if Some(workspace_id.as_str()) == active_workspace => {
                        Some((area_id.clone(), transition.rect()))
                    }
                    _ => None,
                });
        let focus_area = transition_project
            .as_ref()
            .map(|(area_id, _)| area_id.as_str())
            .or(self.desktop_workspace.expanded_area_id.as_deref());
        let focus_session = active_workspace
            .zip(focus_area)
            .and_then(|(workspace_id, area_id)| {
                self.desktop_workspace
                    .sessions
                    .get(&(workspace_id.to_owned(), area_id.to_owned()))
                    .cloned()
            });
        let returning =
            transition_project.is_some() && self.desktop_workspace.expanded_area_id.is_none();
        let workspace_name = self
            .desktop_workspace
            .active()
            .map(|workspace| workspace.name.clone())
            .unwrap_or_else(|| "Workspace".to_string());
        if transition_project.is_none() && workspace_frame.is_none() {
            if let Some(session) = focus_session.clone() {
                return div()
                    .id("desktop-workspace-world")
                    .debug_selector(|| "desktop-workspace-world".to_string())
                    .absolute()
                    .inset_0()
                    .child(self.render_workspace_backdrop())
                    .child(
                        div()
                            .id("desktop-project-expanded")
                            .debug_selector(|| "desktop-project-expanded".to_string())
                            .absolute()
                            .inset_0()
                            .child(self.render_desktop_project_world(
                                session,
                                workspace_name,
                                None,
                                window,
                            )),
                    );
            }
        }
        let areas = self.desktop_workspace.areas.clone();
        let workspace_id = self.desktop_workspace.active_id.clone().unwrap_or_default();
        let area_ids: Vec<_> = areas.iter().map(|area| area.id.clone()).collect();
        let area_rects = self.desktop_workspace.area_rects.clone();
        let zoomed_area_id = transition_project
            .as_ref()
            .map(|(area_id, _)| area_id.clone());
        let capture_workspace_id = workspace_id.clone();
        if let Some(scroll) = self.desktop_overview_scroll.get(&workspace_id) {
            scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        }
        let is_empty = areas.is_empty();
        // A moving camera scales the settled layout; its frame must never trigger a breakpoint.
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height);
        let workspace_frame = workspace_frame.map(|frame| {
            let full = canvas::CanvasRect::new(0.0, 0.0, width, height);
            let source = self
                .desktop_workspace
                .workspace_motion_rects
                .get(&workspace_id);
            let chrome_scale = source.map_or(0.0, |source| {
                (frame.size.x / source.size.x)
                    * super::scene::overview_chrome_opacity(frame, *source, full)
            });
            super::scene::workspace_content_rect(frame, full.size, chrome_scale)
        });
        let scene =
            self.desktop_scene_frame(canvas::CanvasRect::new(0.0, 0.0, width, height), false);
        let scale = workspace_frame
            .map(|frame| frame.size.x / width)
            .unwrap_or(scene.size.x / width);
        let capture_bounds = self.desktop_workspace.transition.is_none();
        let capture_project_bounds = transition_project.is_none();
        let area_layout = self.desktop_project_rects(
            &workspace_id,
            canvas::Vec2::new(width, height),
            areas.iter().map(|area| area.id.as_str()),
        );
        let receipts = self
            .desktop_workspace
            .dispatch_receipts
            .get(&workspace_id)
            .cloned()
            .unwrap_or_default();
        let overview = div()
            .id("desktop-workspace-overview")
            .debug_selector(|| "desktop-workspace-overview".to_string())
            .absolute()
            .left(px(scene.origin.x))
            .top(px(scene.origin.y))
            .w(px(width * scale))
            .h(px(height * scale))
            .flex()
            .flex_col()
            .when(capture_bounds, |this| {
                this.bg(rgba(theme.graphite)).child(
                    div()
                        .id("desktop-workspace-overview-wallpaper")
                        .absolute()
                        .inset_0()
                        .child(self.render_workspace_backdrop()),
                )
            })
            .child(
                div()
                    .id("desktop-project-area-scroll")
                    .debug_selector(|| "desktop-project-area-scroll".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when(is_empty, |this| {
                        this.child(
                            div()
                                .pt(px(48.0 * scale))
                                .px(px(40.0 * scale))
                                .child(self.desktop_empty_canvas(scale, true, cx)),
                        )
                    })
                    .when(!is_empty, |this| {
                        this.child(
                            div()
                                .relative()
                                .w(px(width * scale))
                                .h(px(height * scale))
                                .children(areas.into_iter().zip(area_layout).enumerate().map(
                                    |(index, (area, rect))| {
                                        let slot = div()
                                            .absolute()
                                            .left(px(rect.origin.x * scale))
                                            .top(px(rect.origin.y * scale))
                                            .w(px(rect.size.x * scale))
                                            .h(px(rect.size.y * scale));
                                        slot.child(self.desktop_project_area_card(
                                            index,
                                            area,
                                            &workspace_id,
                                            rect.size.x * scale,
                                            rect.size.y * scale,
                                            scale,
                                            window,
                                            cx,
                                        ))
                                        .into_any_element()
                                    },
                                ))
                                .on_children_prepainted(move |children, _, _| {
                                    if capture_project_bounds {
                                        let mut rects = area_rects
                                            .lock()
                                            .expect("project bounds mutex poisoned");
                                        for (area_id, bounds) in
                                            area_ids.iter().zip(children.iter())
                                        {
                                            let mut rect =
                                                super::camera::canvas_rect_from_bounds(*bounds);
                                            if let Some(frame) = workspace_frame {
                                                rect = super::scene::project_rect(
                                                    frame,
                                                    canvas::CanvasRect::new(
                                                        0.0, 0.0, width, height,
                                                    ),
                                                    rect,
                                                );
                                            }
                                            rects.insert(
                                                (capture_workspace_id.clone(), area_id.clone()),
                                                rect,
                                            );
                                        }
                                    }
                                }),
                        )
                    }),
            )
            .when_some(self.desktop_workspace.error.clone(), |this, error| {
                this.child(
                    div()
                        .px(px(40.0))
                        .pb(px(12.0))
                        .text_color(rgba(theme.destructive))
                        .child(error),
                )
            })
            .when(!receipts.is_empty(), |this| {
                this.child(
                    div()
                        .id("desktop-norma-receipts")
                        .px(px(40.0))
                        .pb(px(8.0))
                        .flex()
                        .justify_center()
                        .gap(px(8.0))
                        .overflow_scroll()
                        .children(receipts.into_iter().enumerate().map(|(index, receipt)| {
                            div()
                                .id(("desktop-norma-receipt", index))
                                .debug_selector(move || format!("desktop-norma-receipt-{index}"))
                                .px(px(9.0))
                                .py(px(5.0))
                                .rounded(px(7.0))
                                .bg(rgba(theme.dock))
                                .text_size(px(11.0))
                                .text_color(rgba(if receipt.submitted {
                                    GLASS_TEXT_SECONDARY
                                } else {
                                    theme.destructive
                                }))
                                .child(format!(
                                    "{} / {} · {}",
                                    receipt.project_name, receipt.agent_name, receipt.status
                                ))
                        })),
                )
            });
        let project_layer = transition_project
            .zip(focus_session)
            .map(|((_, frame), session)| {
                self.render_desktop_project_transition_layer(
                    session,
                    workspace_name.clone(),
                    frame,
                    returning,
                    window,
                    cx,
                )
            });
        let mut world = div()
            .id("desktop-workspace-world")
            .debug_selector(|| "desktop-workspace-world".to_string())
            .absolute();
        if zoomed_area_id.is_some() {
            world = world.child(
                div()
                    .id("desktop-project-receding-wallpaper")
                    .debug_selector(|| "desktop-project-receding-wallpaper".to_string())
                    .absolute()
                    .inset_0()
                    .child(self.render_workspace_backdrop()),
            );
        }
        world = world.child(overview);
        if let Some(layer) = project_layer {
            world = world.child(layer);
        }
        world = if let Some(frame) = workspace_frame {
            world
                .left(px(frame.origin.x))
                .top(px(frame.origin.y))
                .w(px(frame.size.x))
                .h(px(frame.size.y))
                .overflow_hidden()
        } else {
            world.inset_0()
        };
        self.clip_desktop_directory_workspace(world, window)
    }
}
