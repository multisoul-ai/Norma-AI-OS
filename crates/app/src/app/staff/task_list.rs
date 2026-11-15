use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Stateful, Window};
use norma::protocol::{staff_chip_counts, staff_visible_tasks, Task, TaskDisplayStatus};
use ui::component::{
    button::{Button, ButtonVariants},
    input::Input,
    Disableable, Icon, IconName, Selectable, Sizable, StyledExt,
};
use ui::i18n::text as t;

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
        let counts = staff_chip_counts(&owned, project_matches);
        let light = self.theme.is_light();
        let (field, raised) = if light {
            (0xececf0ff, 0xffffffff)
        } else {
            (0x252528ff, 0x3a3a3eff)
        };
        let need_color =
            super::task_row::sidebar_status_color(TaskDisplayStatus::NeedsYou, light, theme);
        // 分段控件：整体一块底，选中项浮起一块。
        let chips = div().px(px(8.)).pb(px(12.)).child({
            let mut seg = div()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(10.))
                .bg(rgba(field));
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
                let selected = self.staff.filter == status;
                let variant = ui::component::button::ButtonCustomVariant::new(cx)
                    .color(rgba(if selected { raised } else { 0 }).into())
                    .foreground(
                        rgba(if selected {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        })
                        .into(),
                    )
                    .hover(
                        rgba(if selected {
                            raised
                        } else {
                            ui::icons::with_alpha(theme.text_primary, 16)
                        })
                        .into(),
                    )
                    .active(rgba(raised).into());
                let count_color = if key == "needs" && count > 0 {
                    need_color
                } else {
                    theme.text_tertiary
                };
                // 外层等分宽度，避免按钮按文字内容撑出不等宽（英文标签更长）。
                seg = seg.child(
                    div().flex_1().min_w_0().child(
                        Button::new(gpui::SharedString::from(format!("staff-filter-{key}")))
                            .debug_selector(move || format!("staff-filter-{key}"))
                            .custom(variant)
                            .small()
                            .w_full()
                            .h(px(30.))
                            .px(px(6.))
                            .rounded(px(8.))
                            .justify_center()
                            .selected(selected)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(13.))
                                    .font_medium()
                                    .child(label)
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .when(key == "needs" && count > 0, |c| c.font_bold())
                                            .text_color(rgba(count_color))
                                            .child(count.to_string()),
                                    ),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.staff.filter = status;
                                app.notify_staff(cx);
                            })),
                    ),
                );
            }
            seg
        });
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
            project_matches,
        );
        let visible = visible_tasks.len();
        for (status, tasks) in norma::protocol::staff_status_groups(&visible_tasks) {
            groups = groups.child(super::task_row::group_header(status, tasks.len(), theme));
            for task in tasks {
                groups = groups.child(self.staff_task_row(task, project_name, list_w - 90., cx));
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
                        .custom(
                            ui::component::button::ButtonCustomVariant::new(cx)
                                .color(rgba(if light { 0x007affff } else { 0x0a84ffff }).into())
                                .foreground(rgba(0xffffffff).into())
                                .hover(rgba(if light { 0x1a86ffff } else { 0x2b94ffff }).into())
                                .active(rgba(if light { 0x0066d6ff } else { 0x0a72e0ff }).into()),
                        )
                        .w_full()
                        .h(px(40.))
                        .px(px(16.))
                        .rounded(px(12.))
                        .justify_center()
                        .icon(Icon::empty().path(crate::assets::TOOLBAR_PLUS_ASSET))
                        .text_size(px(13.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_medium()
                                .text_color(rgba(0xffffffff))
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
                        .h(px(40.))
                        .pl(px(12.))
                        .pr(px(6.))
                        .rounded(px(12.))
                        .bg(rgba(field))
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
