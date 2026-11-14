use super::archive::StaffArchiveNotice;
use super::{view::status_label, *};
use gpui::{div, prelude::*, px, rgba, Div, Stateful, Window};
use norma::protocol::{staff_chip_counts, staff_visible_tasks, Task, TaskDisplayStatus};
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
        let list_w = super::sidebar_width(window).max(280.);
        let owned: Vec<_> = self.staff.tasks.values().cloned().collect();
        let project_filter = self.staff.project_filter.clone();
        let task_projects = self.staff.task_projects.clone();
        let project_matches = |task: &Task| {
            project_filter.as_ref().is_none_or(|area| {
                task_projects
                    .get(&task.id)
                    .is_some_and(|(owner, _)| owner == area)
            })
        };
        let counts = staff_chip_counts(&owned, &project_matches);
        let mut chips = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .pb(px(12.));
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
            let count = match key {
                "needs" => counts.needs_you,
                "running" => counts.running,
                _ => counts.all,
            };
            // 外层等分宽度，避免按钮按文字内容撑出不等宽（英文标签更长）。
            chips = chips.child(
                div().flex_1().min_w_0().child(
                    Button::new(gpui::SharedString::from(format!("staff-filter-{key}")))
                        .debug_selector(move || format!("staff-filter-{key}"))
                        .custom(self.staff_button_style(self.staff.filter == status, cx))
                        .small()
                        .w_full()
                        .h(px(32.))
                        .px(px(8.))
                        .text_size(px(12.))
                        .rounded(px(16.))
                        .justify_center()
                        .border_1()
                        .border_color(rgba(if self.staff.filter == status {
                            0
                        } else {
                            self.staff_line()
                        }))
                        .selected(self.staff.filter == status)
                        .child(div().text_size(px(12.)).child(format!("{label} {count}")))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.staff.filter = status;
                            app.notify_staff(cx);
                        })),
                ),
            );
        }
        let mut groups = div()
            .id("staff-task-groups")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(12.));
        let visible_tasks = staff_visible_tasks(
            &owned,
            self.staff.archive_view,
            self.staff.filter,
            &self.staff.query,
            &project_matches,
        );
        let visible = visible_tasks.len();
        for task in visible_tasks {
            let status = task.display_status();
            let color = status_color(status, theme);
            {
                let id = task.id.clone();
                let group_name = gpui::SharedString::from(format!("staff-row-{id}"));
                let pending_questions = task.pending_question_count();
                let agents = task
                    .workers
                    .iter()
                    .map(|worker| worker.agent.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
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
                            .group(group_name.clone())
                            .relative()
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
                            .items_start()
                            .gap(px(12.))
                            .selected(self.staff.selected.as_deref() == Some(&id))
                            .disabled(self.staff.busy)
                            .child(
                                div()
                                    .mt(px(6.))
                                    .w(px(8.))
                                    .h(px(8.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(rgba(color)),
                            )
                            .child(
                                // Button content shrink-wraps; fixed width keeps status right-aligned.
                                div()
                                    .w(px(list_w - 64.))
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_size(px(14.))
                                                    .font_medium()
                                                    .text_ellipsis()
                                                    .child(task.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .text_size(px(11.))
                                                    .text_color(rgba(color))
                                                    .group_hover(group_name.clone(), |style| {
                                                        style.opacity(0.)
                                                    })
                                                    .child(if pending_questions > 0 {
                                                        format!(
                                                            "{} · {}",
                                                            status_label(status),
                                                            ui::i18n::format(
                                                                "staff.pending_questions",
                                                                &[(
                                                                    "count",
                                                                    pending_questions.to_string()
                                                                )]
                                                            )
                                                        )
                                                    } else {
                                                        status_label(status).to_owned()
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .text_size(px(12.))
                                            .text_color(rgba(theme.text_secondary))
                                            .text_ellipsis()
                                            .child(subtitle),
                                    ),
                            )
                            .when(!self.staff.archive_view, |row| {
                                let archive_id = id.clone();
                                let title = task.title.clone();
                                let selector_id = archive_id.clone();
                                row.child(
                                    Button::new(gpui::SharedString::from(format!(
                                        "staff-archive-row-{selector_id}"
                                    )))
                                    .debug_selector(move || {
                                        format!("staff-archive-row-{selector_id}")
                                    })
                                    .custom(self.staff_button_style(false, cx))
                                    .absolute()
                                    .right(px(8.))
                                    .top(px(8.))
                                    .w(px(32.))
                                    .h(px(32.))
                                    .rounded(px(16.))
                                    .opacity(0.)
                                    .group_hover(group_name.clone(), |style| style.opacity(1.))
                                    .focus(|style| style.opacity(1.))
                                    .in_focus(|style| style.opacity(1.))
                                    .tooltip(ui::i18n::format(
                                        "staff.archive_named",
                                        &[("title", title.clone())],
                                    ))
                                    .icon(Icon::empty().path("icons/archive.svg"))
                                    .disabled(self.staff.busy)
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        cx.stop_propagation();
                                        let task = app.staff.tasks.get(&archive_id);
                                        let active =
                                            task.is_some_and(|task| task.archive_note_active());
                                        let title = task
                                            .map(|task| task.title.clone())
                                            .unwrap_or_else(|| title.clone());
                                        app.request_staff_archive(
                                            archive_id.clone(),
                                            true,
                                            StaffArchiveNotice::Archived { title, active },
                                            window,
                                            cx,
                                        );
                                    })),
                                )
                            })
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
                    .child(
                        if self.staff.archive_view && self.staff.query.trim().is_empty() {
                            t("staff.archive_empty")
                        } else {
                            t("staff.no_matches")
                        },
                    )
                    .when(
                        self.staff.archive_view && self.staff.query.trim().is_empty(),
                        |empty| empty.child(div().pt(px(4.)).child(t("staff.archive_empty_hint"))),
                    ),
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
                div().px(px(8.)).pt(px(8.)).pb(px(12.)).child(
                    Button::new("staff-new-task")
                        .debug_selector(|| "staff-new-task".into())
                        .custom(self.staff_tonal_style(cx))
                        .w_full()
                        .h(px(40.))
                        .px(px(16.))
                        .rounded(px(20.))
                        .justify_center()
                        .icon(Icon::empty().path(crate::assets::TOOLBAR_PLUS_ASSET))
                        .text_size(px(13.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_medium()
                                .text_color(rgba(self.staff_tonal_colors().1))
                                .child(t("staff.new_task")),
                        )
                        .disabled(self.staff.busy)
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.select_staff_task(None, window, cx)
                        })),
                ),
            )
            .child(
                // One Material-style search bar: query input plus an inline project filter.
                div().px(px(8.)).pb(px(12.)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .h(px(44.))
                        .pl(px(14.))
                        .pr(px(6.))
                        .rounded_full()
                        .bg(rgba(theme.panel_high))
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(search)
                                    .appearance(false)
                                    .prefix(Icon::new(IconName::Search).with_size(px(16.)))
                                    .text_size(px(13.))
                                    .cleanable(true),
                            ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(1.))
                                .h(px(20.))
                                .bg(rgba(self.staff_line())),
                        )
                        .child(self.staff_project_picker(false, cx)),
                ),
            )
            .child(if self.staff.archive_view {
                self.staff_archive_bar(visible, cx)
            } else {
                chips
            })
            .child(groups)
            .when(!self.staff.archive_view, |list| {
                list.child(self.staff_archive_link(cx))
            })
    }
}
