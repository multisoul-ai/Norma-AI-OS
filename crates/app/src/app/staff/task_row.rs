//! 侧栏任务行：头像带状态点，标题和任务信息分两行；按状态分组。
use super::archive::StaffArchiveNotice;
use super::list_type::{text, AVATAR, BODY, CAPTION, HIT, SPACE_1, SPACE_2, SPACE_3};
use super::task_time::{avatar_color, avatar_initial, task_time_now};
use super::{view::status_label, *};
use gpui::{div, prelude::*, px, rgba, Div};
use norma::protocol::{Task, TaskDisplayStatus};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Icon, Sizable, StyledExt,
};
use ui::i18n::text as t;
use ui::icons::with_alpha;

const ROW_HEIGHT: f32 = 64.;

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

pub(super) fn group_header(status: TaskDisplayStatus, count: usize, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(SPACE_2))
        .px(px(SPACE_3))
        .pt(px(SPACE_3 + SPACE_1))
        .pb(px(SPACE_2))
        .child(
            text(CAPTION)
                .font_semibold()
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
                .right_0()
                .bottom_0()
                .size(px(11.))
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
        row_width: f32,
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
        let context = if project.is_empty() {
            who
        } else {
            format!("{project} · {who}")
        };
        // 选中态仍走 Button 的 hover / active 分支，以便指针移入或按下时也有反馈。
        let (tint, hover, pressed) = if selected {
            (
                with_alpha(theme.accent_cyan, 46),
                with_alpha(theme.accent_cyan, 66),
                with_alpha(theme.accent_cyan, 87),
            )
        } else {
            (
                0,
                with_alpha(theme.text_primary, 18),
                with_alpha(theme.text_primary, 34),
            )
        };
        let variant = ButtonCustomVariant::new(cx)
            .color(rgba(tint).into())
            .foreground(rgba(theme.text_primary).into())
            .hover(rgba(hover).into())
            .active(rgba(pressed).into());

        // Button 的内部标签按内容收缩，显式限制内容宽度以固定头像与操作的两侧留白。
        let mut content = div()
            .w(px(row_width - 2. * SPACE_3))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .child(avatar(task, color, theme.graphite))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(SPACE_1))
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(SPACE_2))
                            .child(
                                text(BODY)
                                    .flex_1()
                                    .min_w_0()
                                    .font_medium()
                                    .text_color(rgba(theme.text_primary))
                                    .truncate()
                                    .child(task.title.clone()),
                            )
                            .child(
                                text(CAPTION)
                                    .flex_none()
                                    .text_color(rgba(theme.text_secondary))
                                    .child(task_time_now(task.updated_at)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(SPACE_2))
                            .when(pending > 0, |line| {
                                line.child(
                                    text(CAPTION)
                                        .flex_shrink_0()
                                        .text_color(rgba(theme.accent_warm))
                                        .child(ui::i18n::format(
                                            "staff.pending_questions",
                                            &[("count", pending.to_string())],
                                        )),
                                )
                            })
                            .child(
                                text(CAPTION)
                                    .min_w_0()
                                    .truncate()
                                    .text_color(rgba(theme.text_secondary))
                                    .child(context),
                            ),
                    ),
            );
        if !self.staff.archive_view {
            let archive_id = id.clone();
            let selector_id = id.clone();
            // 预留操作位，悬停和键盘聚焦只改图标可见性。
            content = content.child(
                Button::new(gpui::SharedString::from(format!(
                    "staff-archive-row-{selector_id}"
                )))
                .debug_selector(move || format!("staff-archive-row-{selector_id}"))
                .custom(self.staff_button_style(false, cx))
                .small()
                .flex_none()
                .size(px(HIT))
                .rounded_full()
                .opacity(0.)
                .group_hover(group.clone(), |style| style.opacity(1.))
                .focus(|style| style.opacity(1.))
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
        Button::new(gpui::SharedString::from(format!("staff-task-{id}")))
            .debug_selector({
                let id = id.clone();
                move || format!("staff-task-{id}")
            })
            .group(group)
            .custom(variant)
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(SPACE_3))
            .mt(px(SPACE_1))
            .rounded(px(SPACE_3))
            .border_0()
            .when(!self.staff.busy, |row| row.cursor_pointer())
            .when(self.staff.busy, |row| row.opacity(0.55))
            .disabled(self.staff.busy)
            .child(content)
            .on_click(cx.listener(move |app, _, window, cx| {
                app.select_staff_task(Some(id.clone()), window, cx)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, AppContext, Entity, Modifiers, TestAppContext, Window};

    /// 场景：鼠标经过幕僚任务行时，归档提示应正常绘制。
    /// 数据构造：一条有长标题的未归档任务，列表宽 280px，左右各留 8px。
    /// 执行过程：绘制任务行 → 将鼠标移到行中心再重绘 → 移到行下方 1px（行外）再重绘。
    /// 预期结果：双行任务至少 64px 高（为两行文字和 36px 操作留出内边距），归档按钮完全在行内，
    /// 悬停和离开均可完成绘制，任务行和归档按钮的边界保持稳定。
    #[test]
    fn staff_task_row_paints_when_hovered() {
        struct TaskRowHost(Entity<MultisoulApp>, Task);

        impl Render for TaskRowHost {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .pt(px(SPACE_3))
                    .w(px(280.))
                    .px(px(SPACE_2))
                    .child(self.0.update(cx, |app, cx| {
                        app.staff_task_row(
                            &self.1,
                            "Project with a longer name",
                            280. - 2. * SPACE_2,
                            cx,
                        )
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
            TaskRowHost(
                app,
                Task::new("hover", "w", "p", "Archive hover with a much longer title"),
            )
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let row = cx
            .debug_bounds("staff-task-hover")
            .expect("initial task row");
        let archive = cx
            .debug_bounds("staff-archive-row-hover")
            .expect("initial archive hit target");
        assert!(
            row.size.height >= px(64.),
            "two-line task row must provide at least 64px of height, got {row:?}"
        );
        assert!(
            row.top() < archive.top() && archive.bottom() < row.bottom(),
            "the archive hit target must fit inside the row with vertical padding"
        );
        assert!(
            row.left() < archive.left() && archive.right() == row.right() - px(SPACE_3),
            "the archive hit target must end one horizontal inset from the row edge"
        );

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
}
