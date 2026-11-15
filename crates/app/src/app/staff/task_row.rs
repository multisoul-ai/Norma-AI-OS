//! 侧栏任务行：头像带状态点、标题加时间、状态加参与者；按状态分组。
use super::archive::StaffArchiveNotice;
use super::task_time::{avatar_color, avatar_initial, task_time_now};
use super::{view::status_label, *};
use gpui::{div, prelude::*, px, rgba, Div};
use norma::protocol::{Task, TaskDisplayStatus};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Icon, Selectable, StyledExt,
};
use ui::i18n::text as t;

/// 侧栏里的状态色：橙（需要你）、青（执行中）、绿（已完成）。
pub(super) fn sidebar_status_color(
    status: TaskDisplayStatus,
    light: bool,
    theme: ui::Theme,
) -> u32 {
    match (status, light) {
        (TaskDisplayStatus::NeedsYou, false) => 0xff9f0aff,
        (TaskDisplayStatus::NeedsYou, true) => 0xd97a00ff,
        (TaskDisplayStatus::Running, false) => 0x64d2ffff,
        (TaskDisplayStatus::Running, true) => 0x0a8fb8ff,
        (TaskDisplayStatus::Done, false) => 0x30d158ff,
        (TaskDisplayStatus::Done, true) => 0x1f9d46ff,
        (TaskDisplayStatus::Waiting, _) => theme.text_tertiary,
        (TaskDisplayStatus::Merging, _) => theme.accent_violet,
    }
}

/// 选中行的底色（蓝，约 20% 不透明度）。
pub(super) fn selected_tint(light: bool) -> u32 {
    if light {
        0x007aff1f
    } else {
        0x0a84ff33
    }
}

pub(super) fn group_header(status: TaskDisplayStatus, count: usize, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(10.))
        .pt(px(14.))
        .pb(px(6.))
        .text_size(px(12.))
        .font_semibold()
        .text_color(rgba(theme.text_secondary))
        .child(status_label(status))
        .child(
            div()
                .font_normal()
                .text_color(rgba(theme.text_tertiary))
                .child(count.to_string()),
        )
}

/// 头像：Agent 名首字加固定底色，右下角是状态点（环色与侧栏底色一致，像从头像上挖出来）。
fn avatar(task: &Task, status_color: u32, ring: u32) -> Div {
    let name = task
        .workers
        .first()
        .map_or(task.title.as_str(), |w| w.agent.as_str());
    div()
        .relative()
        .flex_none()
        .w(px(36.))
        .h(px(36.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(avatar_color(name)))
        .text_size(px(14.))
        .font_semibold()
        .text_color(rgba(0x111111ff))
        .child(avatar_initial(name))
        .child(
            div()
                .absolute()
                .right(px(-3.))
                .bottom(px(-3.))
                .w(px(13.))
                .h(px(13.))
                .rounded_full()
                .border_2()
                .border_color(rgba(ring))
                .bg(rgba(status_color)),
        )
}

impl MultisoulApp {
    pub(super) fn staff_task_row(
        &self,
        task: &Task,
        project_name: &str,
        text_width: f32,
        cx: &mut Context<Self>,
    ) -> Button {
        let theme = self.staff_theme();
        let light = self.theme.is_light();
        let id = task.id.clone();
        let selected = self.staff.selected.as_deref() == Some(&id);
        let status = task.display_status();
        let color = sidebar_status_color(status, light, theme);
        let group = gpui::SharedString::from(format!("staff-row-{id}"));
        let pending = task.pending_question_count();
        let agents = task
            .workers
            .iter()
            .map(|worker| worker.agent.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("、");
        let project = self
            .staff
            .task_projects
            .get(&task.id)
            .map_or(project_name, |(_, name)| name.as_str());
        let who = if agents.is_empty() {
            t("staff.waiting_plan").to_owned()
        } else {
            agents
        };
        let status_text = if pending > 0 {
            format!(
                "{} · {}",
                status_label(status),
                ui::i18n::format("staff.pending_questions", &[("count", pending.to_string())])
            )
        } else {
            status_label(status).to_owned()
        };
        let tint = if selected { selected_tint(light) } else { 0 };
        let variant = ButtonCustomVariant::new(cx)
            .color(rgba(tint).into())
            .foreground(rgba(theme.text_primary).into())
            .hover(
                rgba(if selected {
                    tint
                } else {
                    ui::icons::with_alpha(theme.text_primary, 16)
                })
                .into(),
            )
            .active(rgba(selected_tint(light)).into());
        let (task_id, title) = (id.clone(), task.title.clone());
        let mut row = Button::new(gpui::SharedString::from(format!("staff-task-{id}")))
            .debug_selector({
                let id = id.clone();
                move || format!("staff-task-{id}")
            })
            .group(group.clone())
            .relative()
            .custom(variant)
            .w_full()
            .h(px(60.))
            .px(px(10.))
            .py(px(10.))
            .mt(px(2.))
            .rounded(px(12.))
            .justify_start()
            .gap(px(12.))
            .selected(selected)
            .disabled(self.staff.busy)
            .child(avatar(
                task,
                color,
                if selected { 0x0a2f5eff } else { theme.graphite },
            ))
            .child(
                // Button 内容按内容收缩；固定宽度才能让时间靠右。
                div()
                    .w(px(text_width))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(2.))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_baseline()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .font_medium()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(task.title.clone()),
                            )
                            .child(
                                // 悬停时让位给归档按钮。
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(rgba(theme.text_tertiary))
                                    .group_hover(group.clone(), |style| style.opacity(0.))
                                    .child(task_time_now(task.updated_at)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .text_color(rgba(theme.text_secondary))
                            .child(div().flex_none().text_color(rgba(color)).child(status_text))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(format!("· {who} · {project}")),
                            ),
                    ),
            );
        if !self.staff.archive_view {
            let archive_id = id.clone();
            let selector_id = id.clone();
            row = row.child(
                Button::new(gpui::SharedString::from(format!(
                    "staff-archive-row-{selector_id}"
                )))
                .debug_selector(move || format!("staff-archive-row-{selector_id}"))
                .custom(self.staff_button_style(false, cx))
                .absolute()
                .right(px(4.))
                .top(px(14.))
                .w(px(32.))
                .h(px(32.))
                .rounded(px(16.))
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .focus(|style| style.opacity(1.))
                .in_focus(|style| style.opacity(1.))
                .tooltip(ui::i18n::format("staff.archive_named", &[("title", title)]))
                .icon(Icon::empty().path("icons/archive.svg"))
                .disabled(self.staff.busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.request_staff_archive(
                        archive_id.clone(),
                        true,
                        StaffArchiveNotice::Archived,
                        window,
                        cx,
                    );
                })),
            );
        }
        row.on_click(cx.listener(move |app, _, window, cx| {
            app.select_staff_task(Some(task_id.clone()), window, cx)
        }))
    }
}
