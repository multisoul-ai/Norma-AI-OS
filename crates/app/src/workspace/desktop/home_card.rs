use crate::app::MultisoulApp;
use canvas::{CanvasRect, Vec2};
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, rgba, Context, Div, KeyDownEvent,
    Stateful,
};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    tooltip::Tooltip,
    StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};

use super::home::SURFACE_ALPHA;

impl MultisoulApp {
    pub(super) fn desktop_home_card(
        &self,
        index: usize,
        workspace: storage::DesktopWorkspace,
        size: Vec2,
        full: Vec2,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let featured = index == 0;
        let inline_meta = featured && size.x >= 600.0;
        let panel = super::layout::home_preview_panel_rect(
            CanvasRect {
                origin: Vec2::ZERO,
                size,
            },
            featured,
        );
        let content = super::layout::home_preview_rect(
            CanvasRect {
                origin: Vec2::ZERO,
                size,
            },
            full,
            featured,
        );
        let moving = self.desktop_directory_workspace_id() == Some(workspace.id.as_str());
        let opacity = if moving {
            self.desktop_workspace
                .workspace_motion_rects
                .get(&workspace.id)
                .map_or(1.0, |source| {
                    let frame = self.desktop_scene_frame(
                        CanvasRect {
                            origin: Vec2::ZERO,
                            size: full,
                        },
                        false,
                    );
                    let frame = self
                        .desktop_workspace
                        .transition
                        .as_ref()
                        .filter(|t| t.directory_content.is_none())
                        .map_or(frame, |t| t.rect());
                    super::scene::overview_chrome_opacity(
                        frame,
                        *source,
                        CanvasRect {
                            origin: Vec2::ZERO,
                            size: full,
                        },
                    )
                })
        } else {
            1.0
        };
        let scaled_content = CanvasRect::new(
            content.origin.x * scale,
            content.origin.y * scale,
            content.size.x * scale,
            content.size.y * scale,
        );
        let running = self
            .desktop_workspace
            .sessions
            .iter()
            .filter(|((id, _), _)| id == &workspace.id)
            .any(|(_, session)| session.read(cx).runtime.running_terminal_count() > 0);
        let open_id = workspace.id.clone();
        let keyboard_id = workspace.id.clone();
        let actions_id = workspace.id.clone();
        let rename_id = workspace.id.clone();
        let delete_id = workspace.id.clone();
        let title = workspace.name.clone();
        let actions_open =
            self.desktop_workspace.actions_workspace_id.as_deref() == Some(&workspace.id);
        let meta_left = if featured { 18.0 } else { panel.max().x + 20.0 };
        let border = with_alpha(theme.edge_highlight, 0x80);
        let card = div()
            .id(("desktop-home-card", index))
            .debug_selector(move || format!("desktop-home-card-{index}"))
            .relative()
            .size_full()
            .rounded(px(9.0 * scale))
            .border_1()
            .border_color(rgba(if featured {
                with_alpha(theme.accent, 0x90)
            } else {
                border
            }))
            .bg(linear_gradient(
                165.0,
                linear_color_stop(rgba(with_alpha(theme.panel_high, SURFACE_ALPHA)), 0.0),
                linear_color_stop(rgba(with_alpha(theme.canvas_to, SURFACE_ALPHA)), 1.0),
            ))
            .shadow_md()
            .opacity(opacity)
            .cursor_pointer()
            .tab_index(0)
            .hover(move |s| {
                s.bg(rgba(with_alpha(theme.panel_high, SURFACE_ALPHA)))
                    .border_color(rgba(theme.accent))
            })
            .active(move |s| s.bg(rgba(with_alpha(theme.panel, SURFACE_ALPHA))))
            .focus(move |s| s.border_color(rgba(theme.focused_stroke)).shadow_md())
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_desktop_workspace(open_id.clone(), window, cx)
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    app.open_desktop_workspace(keyboard_id.clone(), window, cx);
                }
            }))
            .child(
                div()
                    .debug_selector(move || format!("desktop-home-preview-panel-{index}"))
                    .absolute()
                    .left(px(panel.origin.x * scale))
                    .top(px(panel.origin.y * scale))
                    .w(px(panel.size.x * scale))
                    .h(px(panel.size.y * scale))
                    .rounded(px(6.0 * scale))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
                    .bg(rgba(with_alpha(theme.canvas_from, 0x28))),
            )
            .when(!moving, |card| {
                if workspace.project_count == 0 {
                    card.child(
                        div()
                            .debug_selector(move || format!("desktop-workspace-content-{index}"))
                            .absolute()
                            .left(px(scaled_content.origin.x - 1.0))
                            .top(px(scaled_content.origin.y - 1.0))
                            .w(px(scaled_content.size.x))
                            .h(px(scaled_content.size.y))
                            .rounded(px(8.0 * scale))
                            .bg(rgba(with_alpha(theme.text_primary, 0x05)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .debug_selector(move || format!("desktop-home-empty-{index}"))
                                    .relative()
                                    .w(px(64.0 * scale))
                                    .h(px(48.0 * scale))
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .w(px(42.0 * scale))
                                            .h(px(32.0 * scale))
                                            .rounded(px(4.0 * scale))
                                            .border_1()
                                            .border_color(rgba(with_alpha(
                                                theme.text_secondary,
                                                0x80,
                                            )))
                                            .bg(rgba(with_alpha(theme.panel_high, 0x30))),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .bottom_0()
                                            .right_0()
                                            .w(px(38.0 * scale))
                                            .h(px(30.0 * scale))
                                            .rounded(px(4.0 * scale))
                                            .border_1()
                                            .border_color(rgba(with_alpha(
                                                theme.text_secondary,
                                                0xb0,
                                            )))
                                            .bg(rgba(with_alpha(theme.panel_high, 0xa0)))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(toolbar_icon(
                                                ToolbarIcon::Plus,
                                                theme.text_secondary,
                                                16.0 * scale,
                                            )),
                                    ),
                            ),
                    )
                } else {
                    card.child(self.desktop_workspace_preview(
                        index,
                        &workspace,
                        scaled_content,
                        full,
                        cx,
                    ))
                }
            })
            .child(
                div()
                    .absolute()
                    .left(px(meta_left * scale))
                    .top(px(if featured {
                        size.y - if inline_meta { 50.0 } else { 64.0 }
                    } else {
                        12.0
                    } * scale))
                    .bottom(px(12.0 * scale))
                    .right(px(if featured && size.x >= 500.0 {
                        168.0
                    } else {
                        44.0
                    } * scale))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(6.0 * scale))
                    .when(inline_meta, |meta| {
                        meta.flex_row()
                            .items_center()
                            .justify_start()
                            .gap(px(16.0 * scale))
                    })
                    .child(
                        div()
                            .id(("desktop-home-title", index))
                            .debug_selector(move || format!("desktop-home-title-{index}"))
                            .min_w(px(0.0))
                            .when(!inline_meta, |title| title.flex_shrink_0())
                            .text_size(px(if featured { 20.0 } else { 18.0 } * scale))
                            .line_height(px(23.0 * scale))
                            .font_semibold()
                            .text_color(rgba(theme.text_primary))
                            .when(featured, |title| title.truncate())
                            .when(!featured, |title| {
                                title.line_clamp(if size.y > 150.0 { 3 } else { 2 })
                            })
                            .tooltip(move |window, cx| {
                                Tooltip::new(title.clone()).build(window, cx)
                            })
                            .child(workspace.name),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(8.0 * scale))
                            .text_size(px(15.0 * scale))
                            .line_height(px(20.0 * scale))
                            .text_color(rgba(theme.text_secondary))
                            .when(running, |line| {
                                line.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0 * scale))
                                        .px(px(6.0 * scale))
                                        .rounded_full()
                                        .text_size(px(12.0 * scale))
                                        .text_color(rgba(theme.success))
                                        .bg(rgba(with_alpha(theme.success, 0x14)))
                                        .child(
                                            div()
                                                .w(px(6.0 * scale))
                                                .h(px(6.0 * scale))
                                                .rounded_full()
                                                .bg(rgba(theme.success)),
                                        )
                                        .child("Running"),
                                )
                            })
                            .child(
                                div()
                                    .debug_selector(move || format!("desktop-home-count-{index}"))
                                    .when(inline_meta, |count| {
                                        count
                                            .border_l_1()
                                            .border_color(rgba(with_alpha(
                                                theme.text_secondary,
                                                0x60,
                                            )))
                                            .pl(px(16.0 * scale))
                                    })
                                    .child(format!("{} projects", workspace.project_count)),
                            ),
                    )
                    .when(workspace.project_count == 0 && !featured, |meta| {
                        meta.child(
                            div()
                                .text_size(px(12.0 * scale))
                                .text_color(rgba(theme.text_tertiary))
                                .child("No projects yet"),
                        )
                    }),
            )
            .when(!self.desktop_directory_zoom_active(), |card| {
                card.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .top(px(if featured { size.y - 49.0 } else { 12.0 }))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            Button::new(("desktop-workspace-actions", index))
                                .debug_selector(move || {
                                    format!("desktop-workspace-actions-{index}")
                                })
                                .label("⋯")
                                .ghost()
                                .compact()
                                .w(px(28.0))
                                .tooltip("Workspace actions")
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id = if actions_open {
                                        None
                                    } else {
                                        Some(actions_id.clone())
                                    };
                                    cx.notify();
                                })),
                        )
                        .when(featured && size.x >= 500.0, |row| {
                            row.child(
                                Button::new("desktop-home-open")
                                    .label("Open")
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(
                                                rgba(with_alpha(theme.panel_high, SURFACE_ALPHA))
                                                    .into(),
                                            )
                                            .foreground(rgba(theme.text_primary).into())
                                            .border(rgba(theme.edge_highlight).into())
                                            .hover(
                                                rgba(with_alpha(theme.panel, SURFACE_ALPHA)).into(),
                                            )
                                            .active(
                                                rgba(with_alpha(theme.accent, SURFACE_ALPHA))
                                                    .into(),
                                            ),
                                    )
                                    .w(px(88.0))
                                    .h(px(36.0))
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        cx.stop_propagation();
                                        app.open_desktop_workspace(
                                            workspace.id.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        }),
                )
            });
        card.when(
            actions_open && !self.desktop_directory_zoom_active(),
            |card| {
                card.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .top(px(if featured { size.y - 144.0 } else { 44.0 }))
                        .w(px(140.0))
                        .p(px(6.0))
                        .rounded(px(10.0))
                        .bg(rgba(with_alpha(theme.panel_high, SURFACE_ALPHA)))
                        .border_1()
                        .border_color(rgba(border))
                        .shadow_lg()
                        .occlude()
                        .flex()
                        .flex_col()
                        .child(
                            Button::new(("desktop-workspace-rename", index))
                                .label("Rename")
                                .ghost()
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id = None;
                                    app.begin_desktop_rename(rename_id.clone(), window, cx);
                                })),
                        )
                        .child(
                            Button::new(("desktop-workspace-delete", index))
                                .label("Delete")
                                .ghost()
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_workspace_id = None;
                                    app.begin_desktop_workspace_delete(delete_id.clone(), cx);
                                })),
                        ),
                )
            },
        )
    }
}
