use super::{
    glass_button_variant,
    preview_canvas::{project_canvas_rect, project_chrome_scale, project_stage_rect, project_tone},
    GLASS_TEXT,
};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use storage::DesktopProjectArea;
use ui::component::button::{Button, ButtonVariants};
use ui::component::StyledExt;
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn desktop_project_area_card(
        &self,
        area_index: usize,
        area: DesktopProjectArea,
        card_size: canvas::Vec2,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let area_id = area.id.clone();
        let workspace_id = area.desktop_workspace_id.clone();
        let moving = self
            .desktop_workspace
            .transition
            .as_ref()
            .filter(|transition| {
                matches!(&transition.subject, super::camera::DesktopCanvasSubject::Project {
                workspace_id: active_workspace, area_id: active_area,
            } if active_workspace == &workspace_id && (active_area == &area_id
                || transition.projects.as_ref().is_some_and(|projects|
                    projects.layers.iter().any(|layer| layer.id == area_id))))
            });
        let shell_opacity = moving.map_or(1.0, |transition| {
            // Every moving project uses the same overview camera. Its live content is
            // drawn once by the switch layer, while all shells return with that camera.
            let source_area = match &transition.subject {
                super::camera::DesktopCanvasSubject::Project { area_id, .. } => area_id,
                _ => &area_id,
            };
            let source = self
                .desktop_workspace
                .area_rects
                .lock()
                .expect("project bounds mutex poisoned")
                .get(&(workspace_id.clone(), source_area.clone()))
                .copied();
            let size = window.viewport_size();
            source.map_or(0.0, |source| {
                let full = super::desktop_scene_rect(f32::from(size.width), f32::from(size.height));
                let mut source = project_canvas_rect(source, full.size, 1.0);
                let mut frame = transition.rect();
                if transition.projects.is_some() {
                    // Sibling panning is not overview progress; only zoom restores the shells.
                    frame.origin = full.origin;
                    source.origin = full.origin;
                }
                super::scene::overview_chrome_opacity(frame, source, full)
            })
        });
        let logical_size = super::desktop_scene_rect(
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().height),
        )
        .size;
        let chrome = project_chrome_scale(card_size, scale);
        let title_height = project_stage_rect(card_size, scale).origin.y;
        let actions_open = self.desktop_workspace.actions_area_id.as_deref() == Some(&area_id);
        let moving_live_canvas = moving.is_some()
            && self
                .desktop_workspace
                .sessions
                .contains_key(&(workspace_id.clone(), area_id.clone()));
        let click_area = area_id.clone();
        let preview_area = area_id.clone();
        let mut card = div()
            .id(gpui::ElementId::Name(
                format!("desktop-project-area-{area_index}").into(),
            ))
            .debug_selector(move || format!("desktop-project-area-{area_index}"))
            .relative()
            .h(px(card_size.y))
            .rounded(px(14.0 * chrome))
            .border_1()
            .border_color(rgba(theme.dock_line))
            .bg(rgba(with_alpha(theme.dock, 0x8c)))
            .opacity(shell_opacity)
            .cursor_pointer()
            .hover(move |style| style.border_color(rgba(theme.focused_stroke)))
            .active(move |style| style.bg(rgba(with_alpha(theme.dock, 0xb0))))
            .tab_index(0)
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                app.expand_desktop_project_area(click_area.clone(), window, cx);
            }))
            .child(
                div()
                    .h(px(title_height))
                    .px(px(8.0 * chrome))
                    .pr(px(32.0 * chrome))
                    .flex()
                    .items_center()
                    .gap(px(7.0 * chrome))
                    .text_color(rgba(GLASS_TEXT))
                    .child(
                        div()
                            .size(px(6.0 * chrome))
                            .flex_none()
                            .rounded_full()
                            .bg(rgba(project_tone(&area.display_name, theme))),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("desktop-project-title-{area_index}"))
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(13.0 * chrome))
                            .font_semibold()
                            .truncate()
                            .child(area.display_name.clone()),
                    ),
            )
            .when(!moving_live_canvas, |this| {
                this.child(
                    self.desktop_project_canvas_preview(
                        &workspace_id,
                        &area_id,
                        card_size,
                        logical_size,
                        scale,
                        cx,
                    )
                    .id(("desktop-project-live", area_index))
                    .debug_selector(move || format!("desktop-project-live-{area_index}"))
                    // Keep the live scene visible, but operate its controls only after expansion.
                    .child(
                        div()
                            .id(("desktop-project-preview-open", area_index))
                            .absolute()
                            .inset_0()
                            .occlude()
                            .cursor_pointer()
                            .hover(move |style| {
                                style.bg(rgba(with_alpha(theme.focused_stroke, 0x18)))
                            })
                            .active(move |style| {
                                style.bg(rgba(with_alpha(theme.focused_stroke, 0x30)))
                            })
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.expand_desktop_project_area(preview_area.clone(), window, cx);
                            })),
                    ),
                )
            });

        if self.desktop_workspace.transition.is_none() {
            let open_id = area_id.clone();
            let codex_id = area_id.clone();
            let claude_id = area_id.clone();
            let remove_id = area_id.clone();
            let actions_id = area_id.clone();
            card = card.child(
                div()
                    .absolute()
                    .top(px(4.0 * chrome))
                    .right(px(8.0 * chrome))
                    .child(
                        Button::new(("desktop-project-actions", area_index))
                            .debug_selector(move || format!("desktop-project-actions-{area_index}"))
                            .label("⋯")
                            .compact()
                            .w(px(20.0 * chrome))
                            .h(px(20.0 * chrome))
                            .text_size(px(14.0 * chrome))
                            .custom(glass_button_variant(theme, cx))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.desktop_workspace.actions_area_id =
                                    if app.desktop_workspace.actions_area_id.as_deref()
                                        == Some(actions_id.as_str())
                                    {
                                        None
                                    } else {
                                        Some(actions_id.clone())
                                    };
                                cx.notify();
                            })),
                    ),
            );
            if actions_open {
                card = card.child(
                    div()
                        .debug_selector(move || format!("desktop-project-menu-{area_index}"))
                        .absolute()
                        .top(px(title_height))
                        .right(px(8.0))
                        .w(px(116.0))
                        .p(px(5.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(theme.dock_line))
                        .bg(rgba(theme.dock))
                        .shadow_lg()
                        .child(
                            Button::new(("desktop-project-open", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-open-{area_index}")
                                })
                                .label("Open")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.expand_desktop_project_area(open_id.clone(), window, cx);
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-new-codex", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-new-codex-{area_index}")
                                })
                                .label("New Codex")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_agent_creation_for_area(
                                        codex_id.clone(),
                                        canvas::AgentProfile::Codex,
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-new-claude", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-new-claude-{area_index}")
                                })
                                .label("New Claude")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_agent_creation_for_area(
                                        claude_id.clone(),
                                        canvas::AgentProfile::Claude,
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-remove", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-remove-{area_index}")
                                })
                                .label("Remove")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_area_remove(remove_id.clone(), cx);
                                })),
                        ),
                );
            }
        }
        if let Some(error) = self.desktop_workspace.area_errors.get(&area_id).cloned() {
            let retry_id = area_id.clone();
            card = card.child(
                div()
                    .id(("desktop-project-unavailable", area_index))
                    .debug_selector(move || format!("desktop-project-unavailable-{area_index}"))
                    .absolute()
                    .left(px(12.0))
                    .right(px(12.0))
                    .bottom(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.destructive))
                            .child(error),
                    )
                    .child(
                        Button::new(("desktop-project-retry", area_index))
                            .debug_selector(move || format!("desktop-project-retry-{area_index}"))
                            .label("Retry")
                            .compact()
                            .custom(glass_button_variant(theme, cx))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.retry_desktop_project_area(retry_id.clone(), window, cx);
                            })),
                    ),
            );
        }
        card
    }
}
