use super::with_alpha;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;
use workspace::{compact_path, RecentWorkspaces, WorkspaceRecord};

impl MultisoulApp {
    pub(super) fn render_recent_projects(&mut self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col();

        if self.workspace_state.recent.records.is_empty() {
            list = list.child(
                div()
                    .h(px(64.0))
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
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.node_stroke, 0x9c)))
                    .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
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
    let project_mark = record
        .display_name
        .split(['-', '_', ' '])
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    div()
        .id(("launchpad-recent-project", index))
        .h(px(64.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.graphite, 0x12)))
        .bg(rgba(0x00000000))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x12))))
        .child(
            div()
                .w(px(34.0))
                .h(px(34.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .bg(rgba(if index == 0 {
                    theme.accent_violet
                } else {
                    theme.canvas_to
                }))
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(if index == 0 {
                    0xffffffff
                } else {
                    theme.text_secondary
                }))
                .child(if project_mark.is_empty() {
                    "•".to_string()
                } else {
                    project_mark
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(record.display_name),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(compact_path(&record.path)),
                ),
        )
        .child(
            div()
                .text_size(px(16.0))
                .text_color(rgba(theme.text_tertiary))
                .child("›"),
        )
        .on_click(
            cx.listener(move |app, _: &ClickEvent, window: &mut Window, cx| {
                app.open_workspace_in_project_window(path.clone(), true, window, cx);
            }),
        )
}
