use super::home::{home_theme, material};
use crate::app::MultisoulApp;
use canvas::{CanvasRect, Vec2};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use ui::component::{Disableable, StyledExt};

impl MultisoulApp {
    pub(super) fn desktop_home_card(
        &self,
        index: usize,
        workspace: storage::DesktopWorkspace,
        size: Vec2,
        full: Vec2,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let focus = window
            .use_keyed_state(("home-card-focus", index), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let within = focus.contains_focused(window, cx);
        let menu_focus = window
            .use_keyed_state("home-menu-focus", cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let actions_open =
            self.desktop_workspace.actions_workspace_id.as_deref() == Some(&workspace.id);
        let busy = self.desktop_workspace.busy;
        let reduce_motion = self.motion.reduce_motion;
        let moving = self.desktop_directory_zoom_active();
        let hovered = window.use_keyed_state(("home-hovered", index), cx, |_, _| false);
        let hover_progress = material::progress(
            ("home-hover-motion", index),
            *hovered.read(cx) && !busy && !moving,
            200,
            reduce_motion,
            window,
            cx,
        );
        let panel = super::layout::home_preview_panel_rect(
            CanvasRect {
                origin: Vec2::ZERO,
                size,
            },
            false,
        );
        let preview = CanvasRect::new(
            panel.origin.x * scale,
            panel.origin.y * scale,
            panel.size.x * scale,
            panel.size.y * scale,
        );
        let running = self
            .desktop_workspace
            .sessions
            .iter()
            .any(|((id, _), session)| {
                id == &workspace.id && session.read(cx).runtime.running_terminal_count() > 0
            });
        let open_id = workspace.id.clone();
        let action_id = workspace.id.clone();
        let mut surface = div()
            .id(("desktop-home-card-surface", index))
            .relative()
            .top(px(if reduce_motion || moving {
                0.0
            } else {
                -2.0 * hover_progress
            }))
            .size_full()
            .rounded(px(22.0 * scale))
            .border(px(scale))
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(theme.panel))
            .shadow_md()
            .when(!busy && !moving, |surface| {
                surface
                    .group_hover("home-card", move |s| {
                        s.bg(rgba(theme.panel_high))
                            .border_color(rgba(0xffffff38))
                            .shadow_lg()
                    })
                    .group_active("home-card", move |s| {
                        s.bg(rgba(theme.node_fill)).top(px(0.0))
                    })
            })
            .child(
                div()
                    .debug_selector(move || format!("desktop-home-preview-panel-{index}"))
                    .absolute()
                    .left(px(preview.origin.x))
                    .top(px(preview.origin.y))
                    .w(px(preview.size.x))
                    .h(px(preview.size.y))
                    .rounded(px(16.0 * scale))
                    .bg(rgba(theme.canvas_to)),
            )
            .when(!moving, |surface| {
                surface.child(if workspace.project_count == 0 {
                    div()
                        .debug_selector(move || format!("desktop-workspace-content-{index}"))
                        .absolute()
                        .left(px(preview.origin.x))
                        .top(px(preview.origin.y))
                        .w(px(preview.size.x))
                        .h(px(preview.size.y))
                        .rounded(px(16.0 * scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .debug_selector(move || format!("desktop-home-empty-{index}"))
                                .w(px(50.0 * scale))
                                .h(px(38.0 * scale))
                                .rounded(px(6.0 * scale))
                                .border_1()
                                .border_color(rgba(0xc4c7cf66))
                                .text_color(rgba(theme.text_secondary))
                                .text_size(px(22.0 * scale))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("+"),
                        )
                } else {
                    self.desktop_home_preview(index, &workspace, preview, full, cx)
                })
            })
            .child(
                div()
                    .absolute()
                    .left(px(16.0 * scale))
                    .right(px(42.0 * scale))
                    .top(px(145.0 * scale))
                    .flex()
                    .flex_col()
                    .gap(px(4.0 * scale))
                    .child(
                        div()
                            .debug_selector(move || format!("desktop-home-title-{index}"))
                            .text_size(px(15.0 * scale))
                            .line_height(px(20.0 * scale))
                            .font_medium()
                            .truncate()
                            .child(workspace.name.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("desktop-home-count-{index}"))
                            .flex()
                            .items_center()
                            .gap(px(5.0 * scale))
                            .text_size(px(11.0 * scale))
                            .text_color(rgba(theme.text_secondary))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .when(running, |line| {
                                line.child(
                                    div()
                                        .size(px(6.0 * scale))
                                        .flex_none()
                                        .rounded_full()
                                        .bg(rgba(theme.success)),
                                )
                            })
                            .child(super::home::home_metadata(
                                workspace.project_count,
                                workspace.last_opened_at_unix_ms,
                            )),
                    ),
            );
        if !moving {
            surface = surface.child(
                div()
                    .absolute()
                    .right(px(6.0))
                    .top(px(153.0))
                    .opacity(if within || actions_open { 1.0 } else { 0.0 })
                    .group_hover("home-card", |s| s.opacity(1.0))
                    .in_focus(|s| s.opacity(1.0))
                    .child(
                        material::button(("desktop-workspace-actions", index), "⋮", theme, cx)
                            .debug_selector(move || format!("desktop-workspace-actions-{index}"))
                            .tab_stop(!self.desktop_workspace.home.search_open)
                            .size(px(36.0))
                            .disabled(busy)
                            .tooltip(ui::i18n::text("workspace.workspace_actions"))
                            .when(actions_open, |button| button.bg(rgba(0xa8c7fa24)))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.desktop_workspace.home.menu_cursor = 0;
                                app.desktop_workspace.actions_workspace_id = if actions_open {
                                    None
                                } else {
                                    Some(action_id.clone())
                                };
                                if !actions_open {
                                    menu_focus.focus(window);
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let card = div()
            .id(("desktop-home-card", index))
            .debug_selector(move || format!("desktop-home-card-{index}"))
            .group("home-card")
            .on_hover(move |value, _, cx| {
                hovered.update(cx, |hovered, cx| {
                    *hovered = *value;
                    cx.notify();
                })
            })
            .relative()
            .size_full()
            .rounded(px(22.0 * scale))
            .text_color(rgba(theme.text_primary))
            .when(!busy, |card| {
                card.track_focus(
                    &focus
                        .tab_index(0)
                        .tab_stop(!self.desktop_workspace.home.search_open),
                )
                .cursor_pointer()
                .active(move |s| s.bg(rgba(theme.node_fill)))
                .focus(move |s| s.shadow(material::ring(theme.focused_stroke)))
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_desktop_workspace(open_id.clone(), window, cx)
                }))
            })
            .opacity(if busy { 0.45 } else { 1.0 })
            .child(surface);
        card.when(actions_open && !moving, |card| {
            card.child(
                div()
                    .absolute()
                    .right(px(6.0))
                    .top(px(size.y + 6.0))
                    .size(px(0.0))
                    .child(gpui::deferred(
                        gpui::anchored()
                            .anchor(gpui::Corner::TopRight)
                            .snap_to_window_with_margin(px(8.0))
                            .child(self.desktop_home_actions(index, workspace.id, window, cx)),
                    )),
            )
        })
    }

    pub(super) fn desktop_home_create_card(&self, rect: CanvasRect, cx: &mut Context<Self>) -> Div {
        let theme = home_theme(self.theme_id);
        div()
            .absolute()
            .left(px(rect.origin.x))
            .top(px(rect.origin.y))
            .w(px(rect.size.x))
            .h(px(rect.size.y))
            .child(
                material::button(
                    "desktop-home-create-card",
                    ui::i18n::text("workspace.new_workspace"),
                    theme,
                    cx,
                )
                .debug_selector(|| "desktop-home-create-card".into())
                .tab_stop(!self.desktop_workspace.home.search_open)
                .w(px(rect.size.x))
                .h(px(rect.size.y))
                .rounded(px(22.0))
                .border_1()
                .border_dashed()
                .border_color(rgba(0xc4c7cf61))
                .bg(rgba(0x10161e52))
                .disabled(self.desktop_workspace.busy)
                .opacity(if self.desktop_workspace.busy {
                    0.45
                } else {
                    1.0
                })
                .on_click(
                    cx.listener(|app, _, window, cx| app.create_desktop_workspace(window, cx)),
                ),
            )
    }
}
