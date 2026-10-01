use super::{glass_button_variant, GLASS_TEXT, GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, KeyDownEvent};
use ui::component::button::Button;
use ui::component::button::ButtonVariants;
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
        let scale = card_width / full_width;
        let moving = self
            .desktop_workspace
            .transition
            .as_ref()
            .filter(|transition| {
                matches!(&transition.subject, super::camera::DesktopCanvasSubject::Workspace(id)
                if id == &workspace.id)
            });
        let shell_opacity = moving.map_or(1.0, |transition| {
            self.desktop_workspace
                .workspace_motion_rects
                .get(&workspace.id)
                .map_or(0.0, |source| {
                    super::scene::overview_chrome_opacity(
                        transition.rect(),
                        *source,
                        canvas::CanvasRect::new(0.0, 0.0, full_width, full_height),
                    )
                })
        });
        let scroll_y = self
            .desktop_overview_scroll
            .get(&workspace.id)
            .map(|scroll| f32::from(scroll.offset().y))
            .unwrap_or(workspace.overview_scroll_y as f32);
        let open_id = workspace.id.clone();
        let keyboard_id = workspace.id.clone();
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
        let area_layout =
            super::layout::project_area_layout(full_width, full_height, preview.len());
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
        div()
            .id(("desktop-workspace-card", index))
            .debug_selector(move || format!("desktop-workspace-card-{index}"))
            .h(px(full_height * scale))
            .mt(px(60.0 * scene_scale))
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
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    app.open_desktop_workspace(keyboard_id.clone(), window, cx);
                    cx.stop_propagation();
                }
            }))
            .when(moving.is_none(), |this| {
                this.child({
                    // Paint in the outer canvas coordinates; the decorative 1px border must not reflow its contents.
                    div()
                        .absolute()
                        .left(px(-1.0))
                        .top(px(-1.0))
                        .w(px(card_width))
                        .h(px(full_height * scale))
                        .rounded(px(17.0 * scene_scale))
                        .overflow_hidden()
                        .when(preview.is_empty(), |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top(px((48.0 + scroll_y) * scale))
                                    .left(px(40.0 * scale))
                                    .right(px(40.0 * scale))
                                    .child(self.desktop_empty_canvas(scale, false, cx)),
                            )
                        })
                        .when(!preview.is_empty(), |this| {
                            this.child(div().absolute().inset_0().children(
                                preview.into_iter().zip(area_layout).enumerate().map(
                                    |(area_index, (area, rect))| {
                                        let area_selector = format!(
                                            "desktop-workspace-preview-{index}-{area_index}"
                                        );
                                        div()
                                            .debug_selector(move || area_selector.clone())
                                            .absolute()
                                            .left(px(rect.origin.x * scale))
                                            .top(px((rect.origin.y + scroll_y) * scale))
                                            .w(px(rect.size.x * scale))
                                            .h(px(rect.size.y * scale))
                                            .rounded(px(5.0))
                                            .overflow_hidden()
                                            .child(self.render_desktop_project_preview(
                                                &workspace.id,
                                                &area,
                                                rect.size.x * scale,
                                                rect.size.y * scale,
                                                scale,
                                                format!(
                                                "desktop-workspace-preview-{index}-{area_index}"
                                            ),
                                                cx,
                                            ))
                                    },
                                ),
                            ))
                        })
                })
            })
            .child(
                div()
                    .absolute()
                    .top(px(-54.0 * scene_scale))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(52.0 * scene_scale))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0 * scene_scale))
                    .text_size(px(13.0 * scene_scale))
                    .child(
                        div()
                            .flex()
                            .min_w(px(0.0))
                            .flex_col()
                            .gap(px(2.0 * scene_scale))
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("desktop-workspace-title-{index}")
                                    })
                                    .font_semibold()
                                    .text_color(rgba(GLASS_TEXT))
                                    .child(workspace.name),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0 * scene_scale))
                                    .child(
                                        div()
                                            .text_size(px(11.0 * scene_scale))
                                            .text_color(rgba(GLASS_TEXT_SECONDARY))
                                            .child(format!("{project_count} projects")),
                                    )
                                    .child(
                                        div()
                                            .w(px(6.0 * scene_scale))
                                            .h(px(6.0 * scene_scale))
                                            .rounded(px(3.0 * scene_scale))
                                            .bg(rgba(if running > 0 {
                                                theme.success
                                            } else {
                                                GLASS_TEXT_MUTED
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0 * scene_scale))
                                            .text_color(rgba(GLASS_TEXT_SECONDARY))
                                            .child(if running > 0 {
                                                format!("{running} live")
                                            } else {
                                                recent_activity
                                                    .unwrap_or_else(|| "Idle".to_string())
                                            }),
                                    ),
                            ),
                    )
                    .when(moving.is_none(), |this| {
                        this.child(
                            Button::new(("desktop-workspace-actions", index))
                                .debug_selector(move || {
                                    format!("desktop-workspace-actions-{index}")
                                })
                                .label("⋯")
                                .compact()
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
                                .label("Rename")
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
                                .label("Delete")
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
}
