use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context};
use ui::component::button::Button;
use ui::component::StyledExt;

impl MultisoulApp {
    pub(super) fn desktop_workspace_card(
        &self,
        index: usize,
        workspace: storage::DesktopWorkspace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let open_id = workspace.id.clone();
        let open_button_id = workspace.id.clone();
        let rename_id = workspace.id.clone();
        let delete_id = workspace.id.clone();
        let project_count = workspace.project_count;
        let preview = self
            .desktop_workspace
            .previews
            .get(&workspace.id)
            .cloned()
            .unwrap_or_default();
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
            .h(px(198.0))
            .flex()
            .flex_col()
            .rounded(px(17.0))
            .border_1()
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(theme.panel_high))
            .cursor_pointer()
            .hover(move |style| {
                style
                    .border_color(rgba(theme.accent_blue))
                    .bg(rgba(theme.panel_high))
            })
            .active(move |style| style.bg(rgba(theme.canvas_to)))
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_desktop_workspace(open_id.clone(), window, cx)
            }))
            .child(
                div()
                    .h(px(125.0))
                    .mx(px(11.0))
                    .mt(px(11.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(rgba(theme.node_stroke))
                    .bg(rgba(theme.canvas_to))
                    .p(px(8.0))
                    .when(preview.is_empty(), |this| {
                        this.flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child("Empty canvas")
                    })
                    .when(!preview.is_empty(), |this| {
                        this.grid().grid_cols(2).gap(px(5.0)).children(
                            preview.into_iter().take(4).map(|area| {
                                div()
                                    .p(px(6.0))
                                    .rounded(px(5.0))
                                    .border_1()
                                    .border_color(rgba(theme.node_stroke))
                                    .bg(rgba(theme.panel_high))
                                    .flex()
                                    .flex_col()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgba(theme.text_primary))
                                            .child(area.name),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgba(theme.text_tertiary))
                                            .child(format!(
                                                "{} cards · {} agents",
                                                area.card_count, area.agent_count
                                            )),
                                    )
                            }),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgba(theme.text_primary))
                                    .child(workspace.name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(format!("{project_count} projects · {running} live")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(5.0))
                            .child(
                                Button::new(("desktop-workspace-open", index))
                                    .debug_selector(move || {
                                        format!("desktop-workspace-open-{index}")
                                    })
                                    .label("Open")
                                    .compact()
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        cx.stop_propagation();
                                        app.open_desktop_workspace(
                                            open_button_id.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new(("desktop-workspace-rename", index))
                                    .debug_selector(move || {
                                        format!("desktop-workspace-rename-{index}")
                                    })
                                    .label("Rename")
                                    .compact()
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        cx.stop_propagation();
                                        app.begin_desktop_rename(rename_id.clone(), window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("desktop-workspace-delete", index))
                                    .debug_selector(move || {
                                        format!("desktop-workspace-delete-{index}")
                                    })
                                    .label("Delete")
                                    .compact()
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.begin_desktop_workspace_delete(delete_id.clone(), cx);
                                    })),
                            ),
                    ),
            )
    }
}
