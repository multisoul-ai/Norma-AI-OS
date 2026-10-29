use super::*;
use ui::component::Disableable;

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn desktop_home_actions(
        &self,
        index: usize,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let progress = material::progress(
            "home-menu-motion",
            true,
            150,
            self.motion.reduce_motion,
            window,
            cx,
        );
        let focus = window
            .use_keyed_state("home-menu-focus", cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let directly_focused = focus.is_focused(window);
        let rename_id = id.clone();
        let delete_id = id.clone();
        let return_focus = window
            .use_keyed_state(("home-card-focus", index), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let menu = div()
            .id("desktop-home-actions-menu")
            .debug_selector(|| "desktop-home-actions-menu".into())
            .w(px(184.0))
            .py(px(6.0))
            .px(px(6.0))
            .rounded(px(12.0))
            .bg(rgba(0x2a2f36ff))
            .occlude()
            .on_click(|_, _, cx| cx.stop_propagation())
            .track_focus(&focus)
            .on_mouse_down_out(cx.listener(|app, _, _, cx| {
                app.desktop_workspace.actions_workspace_id = None;
                cx.notify();
            }))
            .capture_key_down(
                cx.listener(move |app, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            app.desktop_workspace.actions_workspace_id = None;
                            return_focus.focus(window);
                            cx.notify();
                            cx.stop_propagation();
                        }
                        "up" | "down" => {
                            app.desktop_workspace.home.menu_cursor =
                                if event.keystroke.key == "up" { 0 } else { 1 };
                            focus.focus(window);
                            cx.notify();
                            cx.stop_propagation();
                        }
                        "enter" | "space" if focus.is_focused(window) => {
                            if app.desktop_workspace.busy {
                                return;
                            }
                            app.desktop_workspace.actions_workspace_id = None;
                            if app.desktop_workspace.home.menu_cursor == 0 {
                                app.begin_desktop_rename(id.clone(), window, cx);
                            } else {
                                app.begin_desktop_workspace_delete(id.clone(), cx);
                            }
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                }),
            )
            .child(
                material::button(
                    ("desktop-workspace-rename", index),
                    ui::i18n::text("common.rename"),
                    theme,
                    cx,
                )
                .debug_selector(move || format!("desktop-workspace-rename-{index}"))
                .w_full()
                .h(px(44.0))
                .rounded(px(6.0))
                .disabled(self.desktop_workspace.busy)
                .opacity(if self.desktop_workspace.busy {
                    0.45
                } else {
                    1.0
                })
                .when(
                    directly_focused && self.desktop_workspace.home.menu_cursor == 0,
                    |b| b.bg(rgba(0xa8c7fa1f)),
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.desktop_workspace.actions_workspace_id = None;
                    app.begin_desktop_rename(rename_id.clone(), window, cx);
                })),
            )
            .child(div().h(px(1.0)).my(px(6.0)).bg(rgba(0xffffff1a)))
            .child(
                material::button(
                    ("desktop-workspace-delete", index),
                    ui::i18n::text("common.delete"),
                    ui::Theme {
                        text_primary: theme.destructive,
                        ..theme
                    },
                    cx,
                )
                .debug_selector(move || format!("desktop-workspace-delete-{index}"))
                .w_full()
                .h(px(44.0))
                .rounded(px(6.0))
                .disabled(self.desktop_workspace.busy)
                .opacity(if self.desktop_workspace.busy {
                    0.45
                } else {
                    1.0
                })
                .when(
                    directly_focused && self.desktop_workspace.home.menu_cursor == 1,
                    |b| b.bg(rgba(0xf2b8b51f)),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.desktop_workspace.actions_workspace_id = None;
                    app.begin_desktop_workspace_delete(delete_id.clone(), cx);
                })),
            );
        div()
            .id("desktop-home-menu-animation")
            .w(px(184.0))
            .h(px(113.0))
            .rounded(px(12.0))
            .shadow_lg()
            .opacity(progress)
            .child(
                gpui::scene(
                    "desktop-home-menu-scene",
                    gpui::size(px(184.0), px(113.0)),
                    menu,
                )
                .offset(gpui::point(px(184.0 * 0.04 * (1.0 - progress)), px(0.0)))
                .scale(0.96 + 0.04 * progress),
            )
    }
}
