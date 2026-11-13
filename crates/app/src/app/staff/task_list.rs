use super::{view::status_label, *};
use gpui::{div, prelude::*, px, rgba, Div, Focusable, Stateful, Window};
use norma::protocol::TaskDisplayStatus;
use ui::component::{
    button::{Button, ButtonVariants},
    input::Input,
    Disableable, Icon, IconName, Selectable, Sizable, StyledExt,
};
use ui::i18n::text as t;

pub(super) fn status_color(status: TaskDisplayStatus, theme: ui::Theme) -> u32 {
    match status {
        TaskDisplayStatus::NeedsYou => theme.accent_warm,
        TaskDisplayStatus::Running => theme.accent_cyan,
        TaskDisplayStatus::Waiting => theme.text_tertiary,
        TaskDisplayStatus::Merging => theme.accent_violet,
        TaskDisplayStatus::Done => theme.success,
    }
}

impl MultisoulApp {
    pub(super) fn staff_task_list(
        &mut self,
        project_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.staff_theme();
        let search = self
            .staff
            .search
            .as_ref()
            .expect("staff search initialized");
        let mut chips = div().flex().gap(px(6.)).px(px(16.)).pb(px(10.));
        for (key, status, label) in [
            ("all", None, t("staff.all")),
            (
                "needs",
                Some(TaskDisplayStatus::NeedsYou),
                t("staff.needs_you"),
            ),
            (
                "running",
                Some(TaskDisplayStatus::Running),
                t("staff.running"),
            ),
        ] {
            let count = self
                .staff
                .tasks
                .values()
                .filter(|task| {
                    self.staff.project_filter.as_ref().is_none_or(|area| {
                        self.staff
                            .task_projects
                            .get(&task.id)
                            .is_some_and(|(owner, _)| owner == area)
                    })
                })
                .filter(|task| status.is_none_or(|status| task.display_status() == status))
                .count();
            chips = chips.child(
                Button::new(gpui::SharedString::from(format!("staff-filter-{key}")))
                    .debug_selector(move || format!("staff-filter-{key}"))
                    .custom(self.staff_button_style(self.staff.filter == status, cx))
                    .small()
                    .h(px(26.))
                    .px(px(8.))
                    .text_size(px(12.))
                    .rounded(px(16.))
                    .selected(self.staff.filter == status)
                    .child(div().text_size(px(12.)).child(format!("{label} {count}")))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.staff.filter = status;
                        app.notify_staff(cx);
                    })),
            );
        }
        let mut groups = div()
            .id("staff-task-groups")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(12.));
        let query = self.staff.query.trim().to_lowercase();
        let mut visible = 0;
        for status in [
            TaskDisplayStatus::NeedsYou,
            TaskDisplayStatus::Running,
            TaskDisplayStatus::Waiting,
            TaskDisplayStatus::Merging,
            TaskDisplayStatus::Done,
        ] {
            if self.staff.filter.is_some_and(|filter| filter != status) {
                continue;
            }
            let mut tasks: Vec<_> = self
                .staff
                .tasks
                .values()
                .filter(|task| {
                    self.staff.project_filter.as_ref().is_none_or(|area| {
                        self.staff
                            .task_projects
                            .get(&task.id)
                            .is_some_and(|(owner, _)| owner == area)
                    })
                })
                .filter(|task| task.display_status() == status)
                .filter(|task| {
                    query.is_empty()
                        || task.title.to_lowercase().contains(&query)
                        || task
                            .workers
                            .iter()
                            .any(|worker| worker.agent.to_lowercase().contains(&query))
                })
                .cloned()
                .collect();
            tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at));
            if tasks.is_empty() {
                continue;
            }
            visible += tasks.len();
            let color = status_color(status, theme);
            for task in tasks {
                let id = task.id.clone();
                let agents = task
                    .workers
                    .iter()
                    .map(|worker| worker.agent.as_str())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let project_name = self
                    .staff
                    .task_projects
                    .get(&task.id)
                    .map(|(_, name)| name.as_str())
                    .unwrap_or(project_name);
                let subtitle = format!(
                    "{} · {}",
                    if agents.is_empty() {
                        t("staff.waiting_plan")
                    } else {
                        &agents
                    },
                    project_name
                );
                groups =
                    groups.child(
                        Button::new(gpui::SharedString::from(format!("staff-task-{id}")))
                            .debug_selector(move || format!("staff-task-{}", task.id))
                            .custom(self.staff_button_style(
                                self.staff.selected.as_deref() == Some(&id),
                                cx,
                            ))
                            .w_full()
                            .h_auto()
                            .px(px(14.))
                            .py(px(13.))
                            .mt(px(2.))
                            .rounded(px(16.))
                            .justify_start()
                            .gap(px(10.))
                            .selected(self.staff.selected.as_deref() == Some(&id))
                            .disabled(self.staff.busy)
                            .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(rgba(color)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .w_full()
                                            .text_size(px(13.))
                                            .font_medium()
                                            .text_ellipsis()
                                            .child(task.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .text_size(px(11.))
                                            .text_color(rgba(theme.text_secondary))
                                            .flex()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_ellipsis()
                                                    .child(subtitle),
                                            )
                                            .child(div().flex_none().child(status_label(status))),
                                    ),
                            )
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.select_staff_task(Some(id.clone()), window, cx)
                            })),
                    );
            }
        }
        if visible == 0 {
            groups = groups.child(
                div()
                    .p(px(24.))
                    .text_size(px(13.))
                    .text_color(rgba(theme.text_tertiary))
                    .child(t("staff.no_matches")),
            );
        }
        div()
            .id("staff-task-list")
            .debug_selector(|| "staff-task-list".into())
            .w(px(super::sidebar_width(window).max(280.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(rgba(theme.graphite))
            .child(
                div()
                    .px(px(8.))
                    .pt(px(8.))
                    .pb(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Button::new("staff-new-task")
                            .debug_selector(|| "staff-new-task".into())
                            .custom(self.staff_primary_style(cx))
                            .h(px(48.))
                            .px(px(20.))
                            .rounded(px(16.))
                            .justify_start()
                            .icon(Icon::empty().path(crate::assets::TOOLBAR_PLUS_ASSET))
                            .text_size(px(13.))
                            .child(div().text_size(px(13.)).child(t("staff.new_task")))
                            .disabled(self.staff.busy)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.select_staff_task(None, window, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("staff-toggle-search")
                            .icon(IconName::Search)
                            .with_size(px(36.))
                            .custom(self.staff_button_style(self.staff.search_open, cx))
                            .tooltip(t("staff.search"))
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.staff.search_open = !app.staff.search_open;
                                if app.staff.search_open {
                                    if let Some(search) = &app.staff.search {
                                        search.focus_handle(cx).focus(window);
                                    }
                                }
                                app.notify_staff(cx);
                            })),
                    ),
            )
            .when(self.staff.search_open || !query.is_empty(), |list| {
                list.child(
                    div().px(px(16.)).pb(px(10.)).child(
                        Input::new(search)
                            .bg(rgba(theme.panel_high))
                            .rounded(px(18.))
                            .prefix(Icon::new(IconName::Search).with_size(px(14.)))
                            .h(px(34.))
                            .text_size(px(13.))
                            .cleanable(true),
                    ),
                )
            })
            .child(
                div()
                    .px(px(16.))
                    .pb(px(8.))
                    .child(self.staff_project_picker(false, cx)),
            )
            .child(chips)
            .child(groups)
    }
}
