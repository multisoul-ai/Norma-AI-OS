use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent, Stateful};
use ui::component::button::Button;
use ui::component::input::Input;
use ui::component::StyledExt;

impl MultisoulApp {
    pub(crate) fn render_desktop_switcher(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let query = self.desktop_workspace.switcher_query.trim().to_lowercase();
        let workspaces: Vec<_> = self
            .desktop_workspace
            .workspaces
            .iter()
            .filter(|workspace| query.is_empty() || workspace.name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        let input = self.desktop_workspace.switcher_input.clone();
        div()
            .id("desktop-workspace-switcher")
            .debug_selector(|| "desktop-workspace-switcher".to_string())
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000091))
            .child(
                div()
                    .w(px(760.0))
                    .max_h(px(670.0))
                    .flex()
                    .flex_col()
                    .p(px(22.0))
                    .gap(px(16.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(rgba(theme.node_stroke))
                    .bg(rgba(theme.panel_high))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_semibold()
                                    .text_color(rgba(theme.text_primary))
                                    .child("Switch Workspace"),
                            )
                            .child(
                                Button::new("desktop-workspace-switch-close")
                                    .debug_selector(|| "desktop-workspace-switch-close".to_string())
                                    .label("Close")
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.close_desktop_switcher(cx)),
                                    ),
                            ),
                    )
                    .when_some(input, |this, input| this.child(Input::new(&input)))
                    .child(
                        div()
                            .id("desktop-workspace-switcher-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_scroll()
                            .when(workspaces.is_empty(), |this| {
                                this.child(
                                    div()
                                        .py(px(48.0))
                                        .flex()
                                        .justify_center()
                                        .text_color(rgba(theme.text_secondary))
                                        .child("No workspaces match your search."),
                                )
                            })
                            .child(
                                div().grid().grid_cols(3).gap(px(12.0)).children(
                                    workspaces
                                        .into_iter()
                                        .enumerate()
                                        .map(|(index, workspace)| {
                                            let id = workspace.id.clone();
                                            let key_id = id.clone();
                                            let selected =
                                                self.desktop_workspace.active_id.as_deref()
                                                    == Some(id.as_str());
                                            let preview = self
                                                .desktop_workspace
                                                .previews
                                                .get(&id)
                                                .cloned()
                                                .unwrap_or_default();
                                            let running = self
                                                .desktop_workspace
                                                .sessions
                                                .iter()
                                                .filter(|((workspace_id, _), _)| {
                                                    workspace_id == &id
                                                })
                                                .map(|(_, session)| {
                                                    session
                                                        .read(cx)
                                                        .runtime
                                                        .running_terminal_count()
                                                })
                                                .sum::<usize>();
                                            div()
                                                .id(("desktop-workspace-switch", index))
                                                .debug_selector(move || {
                                                    format!("desktop-workspace-switch-{index}")
                                                })
                                                .h(px(156.0))
                                                .flex()
                                                .flex_col()
                                                .justify_between()
                                                .p(px(12.0))
                                                .rounded(px(13.0))
                                                .border_1()
                                                .border_color(rgba(if selected {
                                                    theme.accent_blue
                                                } else {
                                                    theme.node_stroke
                                                }))
                                                .bg(rgba(theme.canvas_to))
                                                .cursor_pointer()
                                                .hover(move |style| {
                                                    style.border_color(rgba(theme.accent_blue))
                                                })
                                                .active(move |style| {
                                                    style.bg(rgba(theme.panel_high))
                                                })
                                                .tab_index(0)
                                                .focus(move |style| {
                                                    style.border_color(rgba(theme.accent_blue))
                                                })
                                                .on_click(cx.listener(move |app, _, window, cx| {
                                                    app.open_desktop_workspace(
                                                        id.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                }))
                                                .on_key_down(cx.listener(
                                                    move |app, event: &KeyDownEvent, window, cx| {
                                                        if matches!(
                                                            event.keystroke.key.as_str(),
                                                            "enter" | "space"
                                                        ) {
                                                            app.open_desktop_workspace(
                                                                key_id.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        }
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .grid()
                                                        .grid_cols(2)
                                                        .gap(px(4.0))
                                                        .children(preview.into_iter().take(4).map(
                                                            |area| {
                                                                div()
                                                                    .p(px(5.0))
                                                                    .rounded(px(5.0))
                                                                    .bg(rgba(theme.panel_high))
                                                                    .text_size(px(10.0))
                                                                    .text_color(rgba(
                                                                        theme.text_secondary,
                                                                    ))
                                                                    .child(area.name)
                                                            },
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .font_semibold()
                                                        .text_color(rgba(theme.text_primary))
                                                        .child(workspace.name),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(10.0))
                                                        .text_color(rgba(theme.text_tertiary))
                                                        .child(format!(
                                                            "{} projects · {} live",
                                                            workspace.project_count, running
                                                        )),
                                                )
                                        }),
                                ),
                            ),
                    ),
            )
    }
}
