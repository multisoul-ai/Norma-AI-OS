use super::with_alpha;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;
use workspace::{compact_path, RecentWorkspaces, WorkspaceRecord};

impl MultisoulApp {
    pub(super) fn render_recent_projects(&mut self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col().gap(px(10.0));

        if self.workspace_state.recent.records.is_empty() {
            list = list.child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(rgba(theme.text_tertiary))
                    .child("No recent projects"),
            );
        } else {
            let records = recent_project_records_for_launchpad(&self.workspace_state.recent);
            list = list.children(
                records
                    .into_iter()
                    .enumerate()
                    .map(|(index, record)| recent_project_row(theme, index, record, cx)),
            );
        }

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(rgba(theme.text_tertiary))
                    .child("Recent projects")
                    .child(format!(
                        "View all ({})",
                        self.workspace_state.recent.records.len()
                    )),
            )
            .child(
                div()
                    .id("launchpad-recent-list-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .child(list),
            )
            .when_some(self.workspace_state.error.clone(), |this, error| {
                this.child(
                    div()
                        .mt(px(2.0))
                        .px(px(12.0))
                        .py(px(9.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.destructive, 0x66)))
                        .bg(rgba(with_alpha(theme.destructive, 0x18)))
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(error),
                )
            })
    }
}

pub(crate) fn recent_project_records_for_launchpad(
    recent: &RecentWorkspaces,
) -> Vec<WorkspaceRecord> {
    recent.records.clone()
}

fn recent_project_row(
    theme: Theme,
    index: usize,
    record: WorkspaceRecord,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let path = record.path.clone();
    div()
        .id(("launchpad-recent-project", index))
        .h(px(36.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .rounded(px(6.0))
        .px(px(if index == 0 { 12.0 } else { 4.0 }))
        .border_1()
        .border_color(rgba(if index == 0 {
            with_alpha(theme.accent_orange, 0x38)
        } else {
            with_alpha(theme.edge_highlight, 0x00)
        }))
        .bg(rgba(if index == 0 {
            with_alpha(theme.accent_orange, 0x10)
        } else {
            with_alpha(theme.edge_highlight, 0x00)
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x12))))
        .child(
            div()
                .w(px(188.0))
                .truncate()
                .text_size(px(14.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_secondary))
                .child(record.display_name),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_right()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(compact_path(&record.path)),
        )
        .child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(px(4.0))
                .bg(rgba(if index == 0 {
                    theme.accent_orange
                } else {
                    with_alpha(theme.edge_highlight, 0x36)
                })),
        )
        .on_click(
            cx.listener(move |app, _: &ClickEvent, window: &mut Window, cx| {
                app.open_workspace_in_project_window(path.clone(), true, window, cx);
            }),
        )
}
