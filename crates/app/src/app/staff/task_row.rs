//! 侧栏任务行：头像带状态点、标题加时间、状态加参与者；按状态分组。
use super::archive::StaffArchiveNotice;
use super::list_type::{text, AVATAR, BODY, CAPTION, HIT, SPACE_1, SPACE_2, SPACE_3};
use super::task_time::{avatar_color, avatar_initial, task_time_now};
use super::{view::status_label, *};
use gpui::{div, prelude::*, px, rgba, Div};
use norma::protocol::{Task, TaskDisplayStatus};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Icon, Selectable,
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

/// 标题列宽度。分组左右各 SPACE_2，行左右各 SPACE_3，头像后一条 SPACE_3。
/// 有归档按钮时再留出命中列和它前面的间隙，文字在按钮左边结束。
pub(super) fn task_text_width(list_w: f32, reserve_archive: bool) -> f32 {
    let archive = if reserve_archive { HIT + SPACE_3 } else { 0. };
    (list_w - SPACE_2 * 2. - SPACE_3 * 2. - AVATAR - SPACE_3 - archive).max(0.)
}

pub(super) fn group_header(status: TaskDisplayStatus, count: usize, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(SPACE_2))
        .px(px(SPACE_3))
        .pt(px(SPACE_3))
        .pb(px(SPACE_2))
        .child(
            text(CAPTION)
                .text_color(rgba(theme.text_secondary))
                .child(status_label(status)),
        )
        .child(
            text(CAPTION)
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
        .w(px(AVATAR))
        .h(px(AVATAR))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(avatar_color(name)))
        .child(
            text(BODY)
                .text_color(rgba(0x111111ff))
                .child(avatar_initial(name)),
        )
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
        let show_archive = !self.staff.archive_view;
        // 悬停只显示右侧归档图标，标题与时间保持原样，行内布局不跳动。
        let title_line = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_baseline()
            .gap(px(SPACE_2))
            .child(
                text(BODY)
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(task.title.clone()),
            )
            .child(
                text(BODY)
                    .flex_none()
                    .text_color(rgba(theme.text_tertiary))
                    .child(task_time_now(task.updated_at)),
            );
        let task_id = id.clone();
        let mut row = Button::new(gpui::SharedString::from(format!("staff-task-{id}")))
            .debug_selector({
                let id = id.clone();
                move || format!("staff-task-{id}")
            })
            .group(group.clone())
            .custom(variant)
            .w_full()
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .mt(px(SPACE_1))
            .rounded(px(SPACE_3))
            .justify_start()
            .gap(px(SPACE_3))
            .selected(selected)
            .disabled(self.staff.busy)
            .child(avatar(
                task,
                color,
                if selected { 0x0a2f5eff } else { theme.graphite },
            ))
            .child(
                div()
                    .w(px(text_width.max(0.)))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(SPACE_1))
                    .child(title_line)
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(SPACE_1))
                            .child(
                                text(CAPTION)
                                    .flex_none()
                                    .text_color(rgba(color))
                                    .child(status_text),
                            )
                            .child(
                                text(CAPTION)
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(format!("· {who} · {project}")),
                            ),
                    ),
            );
        if show_archive {
            let archive_id = id.clone();
            let selector_id = id.clone();
            row = row.child(
                Button::new(gpui::SharedString::from(format!(
                    "staff-archive-row-{selector_id}"
                )))
                .debug_selector(move || format!("staff-archive-row-{selector_id}"))
                .custom(self.staff_button_style(false, cx))
                .flex_none()
                .w(px(HIT))
                .h(px(HIT))
                .rounded(px(HIT / 2.))
                .opacity(0.)
                // 只在悬停这一行时显示。不用聚焦样式：点空白处后三行的按钮会一起亮出来。
                .group_hover(group, |style| style.opacity(1.))
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, AppContext, Entity, Modifiers, TestAppContext, Window};

    /// 场景：鼠标经过幕僚任务行时，归档提示应正常绘制。
    /// 数据构造：一条未归档任务，侧栏宽 280px，标题列按常规归档侧栏计算。
    /// 执行过程：绘制任务行 → 将鼠标移到行中心再重绘 → 移到行下方 1px（行外）再重绘。
    /// 预期结果：悬停和离开均可完成绘制，任务行和归档按钮的边界保持稳定。
    #[test]
    fn staff_task_row_paints_when_hovered() {
        struct TaskRowHost(Entity<MultisoulApp>, Task);

        impl Render for TaskRowHost {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .pt(px(SPACE_3))
                    .w(px(280.))
                    .child(self.0.update(cx, |app, cx| {
                        app.staff_task_row(&self.1, "Project", task_text_width(280., true), cx)
                    }))
            }
        }

        let dir = tempfile::tempdir().expect("isolated project");
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let mut test = TestAppContext::single();
        let (_, cx) = test.add_window_view(|_, cx| {
            let app = cx.new(|cx| {
                MultisoulApp::new_with_pty_factory_and_database_paths(
                    cx,
                    Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                    paths,
                )
            });
            TaskRowHost(app, Task::new("hover", "w", "p", "Archive hover"))
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let row = cx
            .debug_bounds("staff-task-hover")
            .expect("initial task row");
        let archive = cx
            .debug_bounds("staff-archive-row-hover")
            .expect("initial archive hit target");

        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            cx.debug_bounds("staff-task-hover"),
            Some(row),
            "hovering should paint the task row at its original position"
        );
        assert_eq!(
            cx.debug_bounds("staff-archive-row-hover"),
            Some(archive),
            "hovering should paint the archive button at its original hit target"
        );

        cx.simulate_mouse_move(
            point(row.center().x, row.bottom() + px(1.)),
            None,
            Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            cx.debug_bounds("staff-task-hover"),
            Some(row),
            "leaving should restore the regular row without shifting it"
        );
    }

    /// 归档命中列从标题列里扣掉，文字在按钮左边结束。
    ///
    /// 数据构造（list_w = 280，sidebar_width 的桌面宽度）：
    ///   分组左右 SPACE_2：8 * 2 = 16
    ///   行左右 SPACE_3：12 * 2 = 24
    ///   头像 AVATAR = 36，头像后间隙 SPACE_3 = 12
    ///   无按钮：280 - 16 - 24 - 36 - 12 = 192
    ///   有按钮再减 HIT 36 + 间隙 12 = 48 → 144
    ///
    /// 执行过程：
    ///   1. task_text_width(280, false) 只减内边距、头像和一条间隙
    ///   2. task_text_width(280, true) 再减归档命中列
    ///   3. task_text_width(0, true) 宽度不够时落到 0
    ///
    /// 预期结果：
    ///   - 无按钮是 192，不预留命中列
    ///   - 有按钮是 144，比无按钮少的正好是 36 + 12
    ///   - 侧栏宽度为 0 时不是负数
    #[test]
    fn archive_slot_narrows_the_title_column() {
        assert_eq!(
            task_text_width(280., false),
            192.,
            "without the archive button the title column keeps the full remainder"
        );
        assert_eq!(
            task_text_width(280., true),
            144.,
            "the 36px hit target plus its 12px gap must come out of the title column"
        );
        assert_ne!(
            task_text_width(280., true),
            task_text_width(280., false),
            "reserving the archive button must narrow the title column"
        );
        assert_eq!(
            task_text_width(0., true),
            0.,
            "a vanished sidebar must not produce a negative title width"
        );
    }
}
