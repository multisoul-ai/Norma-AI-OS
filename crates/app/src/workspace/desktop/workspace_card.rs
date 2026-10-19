use super::{glass_button_variant, GLASS_TEXT, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context};
use ui::component::button::Button;
use ui::component::button::ButtonVariants;
use ui::component::tooltip::Tooltip;
use ui::component::StyledExt;
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn desktop_workspace_card(
        &self,
        index: usize,
        workspace: storage::DesktopWorkspace,
        card_width: f32,
        viewport: gpui::Size<gpui::Pixels>,
        scene_scale: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.desktop_theme();
        let full_width = f32::from(viewport.width);
        let full_height = f32::from(viewport.height);
        let card_height = full_height * card_width / full_width;
        let content = super::scene::workspace_content_rect(
            canvas::CanvasRect::new(0.0, 0.0, card_width, card_height),
            canvas::Vec2::new(full_width, full_height),
            scene_scale,
        );
        let moving = self
            .desktop_workspace
            .transition
            .as_ref()
            .filter(|transition| {
                matches!(&transition.subject, super::camera::DesktopCanvasSubject::Workspace(id)
                if id == &workspace.id)
                    || (transition.directory_content.is_some()
                        && self.desktop_directory_workspace_id() == Some(workspace.id.as_str()))
            });
        let shell_opacity = moving.map_or(1.0, |transition| {
            if let Some(content) = transition.directory_content {
                let frame = self.desktop_scene_frame(
                    canvas::CanvasRect::new(0.0, 0.0, full_width, full_height),
                    false,
                );
                return ((full_width - frame.size.x) / (full_width - content.size.x).max(1.0))
                    .clamp(0.0, 1.0);
            }
            self.desktop_workspace
                .workspace_motion_rects
                .get(&workspace.id)
                .map_or(0.0, |source| {
                    super::scene::overview_chrome_opacity(
                        transition.rect(),
                        *source,
                        self.desktop_workspace_expanded_rect(viewport),
                    )
                })
        });
        let open_id = workspace.id.clone();
        let actions_id = workspace.id.clone();
        let rename_id = workspace.id.clone();
        let delete_id = workspace.id.clone();
        let project_count = workspace.project_count;
        let actions_open =
            self.desktop_workspace.actions_workspace_id.as_deref() == Some(workspace.id.as_str());
        let preview = self
            .desktop_workspace
            .previews
            .get(&workspace.id)
            .cloned()
            .unwrap_or_default();
        let recent_activity = preview
            .iter()
            .find_map(|area| self.desktop_recent_agent_activity(&workspace.id, &area.id, cx));
        let running = self
            .desktop_workspace
            .sessions
            .iter()
            .filter(|((workspace_id, _), _)| workspace_id == &workspace.id)
            .map(|(_, session)| session.read(cx).runtime.running_terminal_count())
            .sum::<usize>();
        let title_tooltip = ui::i18n::format(
            "workspace.project_count_projects",
            &[
                ("arg0", (workspace.name).to_string()),
                ("project_count", (project_count).to_string()),
                (
                    "arg1",
                    (if running > 0 {
                        ui::i18n::format(
                            "workspace.running_live",
                            &[("running", (running).to_string())],
                        )
                    } else {
                        recent_activity
                            .unwrap_or_else(|| ui::i18n::text("workspace.idle").to_string())
                    })
                    .to_string(),
                ),
            ],
        );
        let show_count = card_width / scene_scale >= 360.0 && workspace.name.chars().count() <= 18;
        div()
            .id(("desktop-workspace-card", index))
            .debug_selector(move || format!("desktop-workspace-card-{index}"))
            .h(px(card_height))
            .min_w(px(0.0))
            .relative()
            .flex()
            .flex_col()
            .rounded(px(17.0 * scene_scale))
            .border_1()
            .border_color(rgba(
                if self.desktop_workspace.active_id.as_deref() == Some(workspace.id.as_str()) {
                    with_alpha(theme.accent_green, 0x90)
                } else {
                    theme.dock_line
                },
            ))
            .bg(rgba(with_alpha(theme.dock, 0x8c)))
            .opacity(shell_opacity)
            .shadow_lg()
            .cursor_pointer()
            .hover(move |style| style.border_color(rgba(theme.accent_blue)))
            .active(move |style| style.bg(rgba(with_alpha(theme.dock, 0xf0))))
            .tab_index(0)
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)).shadow_lg())
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_desktop_workspace(open_id.clone(), window, cx)
            }))
            .when(moving.is_none(), |this| {
                this.child(self.desktop_workspace_preview(
                    index,
                    &workspace,
                    content,
                    canvas::Vec2::new(full_width, full_height),
                    cx,
                ))
            })
            .child(
                div()
                    .debug_selector(move || format!("desktop-workspace-header-{index}"))
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(super::scene::WORKSPACE_TITLE_HEIGHT
                        * scene_scale
                        * shell_opacity))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0 * scene_scale))
                    .px(px(14.0 * scene_scale))
                    .text_size(px(13.0 * scene_scale))
                    .child(
                        div()
                            .id(("desktop-workspace-title", index))
                            .debug_selector(move || format!("desktop-workspace-title-{index}"))
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .font_semibold()
                            .text_color(rgba(GLASS_TEXT))
                            .tooltip(move |window, cx| {
                                Tooltip::new(title_tooltip.clone()).build(window, cx)
                            })
                            .child(workspace.name),
                    )
                    .when(show_count, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0 * scene_scale))
                                .text_color(rgba(GLASS_TEXT_SECONDARY))
                                .child(ui::i18n::format(
                                    "workspace.project_count_projects_2",
                                    &[("project_count", (project_count).to_string())],
                                )),
                        )
                    })
                    .when(running > 0, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .w(px(6.0 * scene_scale))
                                .h(px(6.0 * scene_scale))
                                .rounded(px(3.0 * scene_scale))
                                .bg(rgba(theme.success)),
                        )
                    })
                    .when(moving.is_none(), |this| {
                        this.child(
                            Button::new(("desktop-workspace-actions", index))
                                .debug_selector(move || {
                                    format!("desktop-workspace-actions-{index}")
                                })
                                .label("⋯")
                                .compact()
                                .flex_shrink_0()
                                .w(px(28.0 * scene_scale))
                                .h(px(28.0 * scene_scale))
                                .text_size(px(14.0 * scene_scale))
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id =
                                        if app.desktop_workspace.actions_workspace_id.as_deref()
                                            == Some(actions_id.as_str())
                                        {
                                            None
                                        } else {
                                            Some(actions_id.clone())
                                        };
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .when(actions_open && moving.is_none(), |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .top(px(48.0))
                        .w(px(136.0))
                        .p(px(6.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgba(theme.dock_line))
                        .bg(rgba(theme.dock))
                        .shadow_lg()
                        .child(
                            Button::new(("desktop-workspace-rename", index))
                                .debug_selector(move || format!("desktop-workspace-rename-{index}"))
                                .label(ui::i18n::text("common.rename"))
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id = None;
                                    app.begin_desktop_rename(rename_id.clone(), window, cx);
                                })),
                        )
                        .child(
                            Button::new(("desktop-workspace-delete", index))
                                .debug_selector(move || format!("desktop-workspace-delete-{index}"))
                                .label(ui::i18n::text("common.delete"))
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id = None;
                                    app.begin_desktop_workspace_delete(delete_id.clone(), cx);
                                })),
                        ),
                )
            })
    }

    pub(super) fn desktop_workspace_preview(
        &self,
        index: usize,
        workspace: &storage::DesktopWorkspace,
        content: canvas::CanvasRect,
        full: canvas::Vec2,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let scale = content.size.x / full.x;
        let theme = self.desktop_theme();
        let preview = self
            .desktop_workspace
            .previews
            .get(&workspace.id)
            .cloned()
            .unwrap_or_default();
        let areas = self.desktop_project_rects(
            &workspace.id,
            full,
            preview.iter().map(|area| area.id.as_str()),
        );
        let home = self.desktop_workspace.home_directory;
        let areas = if home {
            super::layout::home_project_preview_layout(content.size, &areas)
        } else {
            areas
                .into_iter()
                .map(|rect| {
                    canvas::CanvasRect::new(
                        rect.origin.x * scale,
                        rect.origin.y * scale,
                        rect.size.x * scale,
                        rect.size.y * scale,
                    )
                })
                .collect()
        };
        let compact_empty = content.size.y < 126.0;
        let tiny_empty = content.size.y < 88.0;
        div()
            .debug_selector(move || format!("desktop-workspace-content-{index}"))
            .absolute()
            .left(px(content.origin.x - if home { 0.0 } else { 1.0 }))
            .top(px(content.origin.y - if home { 0.0 } else { 1.0 }))
            .w(px(content.size.x))
            .h(px(content.size.y))
            .overflow_hidden()
            .when(preview.is_empty(), |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(if compact_empty { 5.0 } else { 8.0 }))
                        .when(!tiny_empty, |this| {
                            this.child(
                                div()
                                    .debug_selector(move || {
                                        format!("desktop-workspace-empty-icon-{index}")
                                    })
                                    .size(px(if compact_empty { 30.0 } else { 42.0 }))
                                    .rounded(px(if compact_empty { 9.0 } else { 13.0 }))
                                    .border_1()
                                    .border_color(rgba(with_alpha(theme.accent_blue, 0x70)))
                                    .bg(rgba(with_alpha(theme.accent_blue, 0x20)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(if compact_empty { 20.0 } else { 26.0 }))
                                    .text_color(rgba(GLASS_TEXT))
                                    .child("+"),
                            )
                        })
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!("desktop-workspace-empty-title-{index}")
                                })
                                .font_semibold()
                                .max_w(px((content.size.x - 16.0).max(1.0)))
                                .text_center()
                                .text_size(px(if compact_empty { 16.0 } else { 18.0 }))
                                .line_height(px(if compact_empty { 21.0 } else { 24.0 }))
                                .text_color(rgba(GLASS_TEXT))
                                .child(ui::i18n::text("workspace.a_canvas_for_your_projects")),
                        )
                        .when(!compact_empty, |this| {
                            this.child(
                            div()
                                .debug_selector(move || {
                                    format!("desktop-workspace-empty-description-{index}")
                                })
                                .max_w(px((content.size.x - 32.0).max(1.0)))
                                .text_center()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(rgba(GLASS_TEXT_SECONDARY))
                                .child(ui::i18n::text(
                                    "workspace.add_a_project_area_to_start_working_with_its_agents",
                                )),
                        )
                        }),
                )
            })
            .children(preview.into_iter().zip(areas).enumerate().map(
                |(area_index, (area, rect))| {
                    let selector = format!("desktop-workspace-preview-{index}-{area_index}");
                    let preview_scale = if home {
                        (rect.size.x / 300.0).min(1.0)
                    } else {
                        scale
                    };
                    div()
                        .absolute()
                        .left(px(rect.origin.x))
                        .top(px(rect.origin.y))
                        .w(px(rect.size.x))
                        .h(px(rect.size.y))
                        .overflow_hidden()
                        .child(
                            self.render_desktop_project_preview(
                                &workspace.id,
                                &area,
                                rect.size,
                                preview_scale,
                                super::desktop_scene_rect(full.x, full.y).size,
                                cx,
                            )
                            .debug_selector(move || selector.clone()),
                        )
                },
            ))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_workspace_card_tests.rs"]
mod tests;
