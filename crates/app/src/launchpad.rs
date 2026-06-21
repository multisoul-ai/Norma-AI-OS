//! Workspace launchpad UI.

use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, ClickEvent, Context, Div, FontWeight, Window,
};
use ui::Theme;
use workspace::{compact_path, WorkspaceRecord};

const LAUNCHPAD_WIDTH_PX: f32 = 560.0;
const ACTION_CARD_W_PX: f32 = 170.0;
const ACTION_CARD_H_PX: f32 = 78.0;

impl MultisoulApp {
    pub(super) fn render_launchpad(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let win_h = f32::from(win.height);
        let left = ((win_w - LAUNCHPAD_WIDTH_PX) * 0.5).max(24.0);
        let top = ((win_h - 360.0) * 0.48).max(96.0);

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .bg(rgba(theme.graphite))
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(LAUNCHPAD_WIDTH_PX))
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(launchpad_brand(theme))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(16.0))
                            .child(
                                action_card(theme, "□", "Open Project", true)
                                    .id("launchpad-open-project")
                                    .on_click(cx.listener(Self::on_launchpad_open_project)),
                            )
                            .child(
                                action_card(theme, "↓", "Clone Repo", false)
                                    .id("launchpad-clone-repo")
                                    .on_click(cx.listener(Self::on_launchpad_clone_todo)),
                            ),
                    )
                    .child(self.render_recent_projects(theme, cx)),
            )
    }

    fn render_recent_projects(&mut self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col().gap(px(10.0));

        if self.workspace_state.recent.records.is_empty() {
            list = list.child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(rgba(theme.text_tertiary))
                    .child("No recent projects"),
            );
        } else {
            let records: Vec<WorkspaceRecord> = self.workspace_state.recent.top(5).to_vec();
            list = list.children(
                records
                    .into_iter()
                    .enumerate()
                    .map(|(index, record)| recent_project_row(theme, index, record, cx)),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
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
            .child(list)
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

    fn on_launchpad_open_project(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_open_project(cx);
    }

    fn on_launchpad_clone_todo(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_state.mark_clone_todo();
        cx.notify();
    }
}

fn launchpad_brand(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .w(px(35.0))
                        .h(px(35.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x80)))
                        .bg(rgba(with_alpha(theme.accent_cyan, 0x22)))
                        .child(
                            div()
                                .m(px(8.0))
                                .w(px(18.0))
                                .h(px(18.0))
                                .rounded(px(5.0))
                                .bg(rgba(theme.text_primary)),
                        ),
                )
                .child(
                    div()
                        .text_size(px(28.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child("MULTISOUL"),
                ),
        )
        .child(
            div()
                .ml(px(47.0))
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child("Workspace"),
        )
}

fn action_card(theme: Theme, icon: &'static str, label: &'static str, enabled: bool) -> Div {
    let text_color = if enabled {
        theme.text_primary
    } else {
        theme.text_tertiary
    };
    div()
        .w(px(ACTION_CARD_W_PX))
        .h(px(ACTION_CARD_H_PX))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(8.0))
        .px(px(14.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x20)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(with_alpha(
            theme.panel,
            if enabled { 0xe8 } else { 0x82 },
        )))
        .text_color(rgba(text_color))
        .cursor_pointer()
        .hover(move |style| {
            if enabled {
                style.bg(rgba(with_alpha(theme.panel_high, 0xe4)))
            } else {
                style
            }
        })
        .child(
            div()
                .text_size(px(19.0))
                .text_color(rgba(if enabled {
                    theme.text_secondary
                } else {
                    theme.text_tertiary
                }))
                .child(icon),
        )
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
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
        .h(px(28.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(20.0))
        .rounded(px(6.0))
        .px(px(2.0))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x10))))
        .child(
            div()
                .w(px(230.0))
                .truncate()
                .text_size(px(15.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .child(record.display_name),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_right()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child(compact_path(&record.path)),
        )
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            app.open_workspace_path(path.clone(), cx);
        }))
}

fn launchpad_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(16.0)),
        blur_radius: px(34.0),
        spread_radius: px(-22.0),
    }]
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
