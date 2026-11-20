use super::worker_remove::RemoveRequest;
use super::{
    view::{outcome_label, status_label},
    *,
};
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{TaskDisplayStatus, Worker, WorkerChip, WorkerOutcome};
use runtime::island::bloub::Mood;
use std::collections::HashMap;
use ui::component::{
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
    StyledExt,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn staff_task_header(
        &self,
        task: &Task,
        project_name: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .px(px(20.))
            .py(px(18.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgba(self.staff_theme().text_secondary))
                            .child(project_name.to_owned()),
                    )
                    .child(
                        div()
                            .text_size(px(21.))
                            .font_semibold()
                            .child(task.title.clone()),
                    ),
            )
            .child(self.staff_archive_header(task, cx))
    }

    pub(super) fn staff_workers(&self, task: &Task, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.staff_theme();
        let mut strip = div()
            .id("staff-workers")
            .debug_selector(|| "staff-workers".into())
            .flex_none()
            .flex()
            .gap(px(8.))
            .overflow_x_scroll()
            .px(px(20.))
            .py(px(12.));
        if task.workers.is_empty() {
            return strip.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba(theme.text_tertiary))
                    .child(t("staff.no_workers")),
            );
        }
        for (index, worker) in workers_for_strip(&task.workers).into_iter().enumerate() {
            let chip = worker.chip();
            let color = match chip {
                WorkerChip::NeedsYou => theme.accent_warm,
                WorkerChip::Running => theme.accent_cyan,
                WorkerChip::Starting => theme.accent_violet,
                WorkerChip::Done => theme.success,
            };
            let status_text = if matches!(
                worker.phase,
                norma::protocol::WorkerPhase::Failed(norma::protocol::WorkerFailure::Restarted)
            ) {
                t("staff.worker_restarted")
            } else {
                worker.outcome().map(outcome_label).unwrap_or(match chip {
                    WorkerChip::Starting => t("staff.worker_starting"),
                    WorkerChip::Running => status_label(TaskDisplayStatus::Running),
                    WorkerChip::NeedsYou => status_label(TaskDisplayStatus::NeedsYou),
                    WorkerChip::Done => status_label(TaskDisplayStatus::Done),
                })
            };
            let node = worker.card_id;
            let (app, task_id, dispatch_id) = (
                cx.weak_entity(),
                task.id.clone(),
                worker.dispatch_id.clone(),
            );
            let running = worker.outcome().is_none();
            let title = format!("{} · {}", worker.agent, worker.engine);
            let (secondary, danger) = (theme.text_secondary, theme.destructive);
            strip = strip.child(
                Button::new(("staff-worker", index))
                    .debug_selector(move || format!("staff-worker-{}", node.unwrap_or_default()))
                    .custom(self.staff_button_style(false, cx))
                    .h(px(36.))
                    .px(px(12.))
                    .rounded_full()
                    .bg(rgba(theme.panel_high))
                    .flex_none()
                    .justify_start()
                    .gap(px(8.))
                    .tooltip(format!("{} · {}", worker.role, t("staff.worker_menu")))
                    .child(crate::island::pet(
                        &worker.agent,
                        &worker.engine,
                        24.,
                        worker_mood(chip, worker.outcome()),
                        !self.motion.reduce_motion,
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_medium()
                            .child(worker.agent.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgba(theme.text_secondary))
                            .child(worker.engine.clone()),
                    )
                    .child(div().w(px(5.)).h(px(5.)).rounded_full().bg(rgba(color)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgba(color))
                            .child(status_text),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(rgba(theme.text_secondary))
                            .child("⋯"),
                    )
                    .dropdown_menu(move |menu, _, _| {
                        worker_menu(
                            menu,
                            &app,
                            (&task_id, &dispatch_id),
                            title.clone(),
                            (secondary, danger),
                            (node, running),
                        )
                    }),
            );
        }
        strip
    }
}

fn workers_for_strip(workers: &[Worker]) -> Vec<&Worker> {
    let mut visible = Vec::new();
    let mut card_positions = HashMap::new();
    for worker in workers {
        if let Some(card_id) = worker.card_id {
            // 派发按顺序追加，复用时让最新派发更新原卡的位置。
            let position = *card_positions.entry(card_id).or_insert(visible.len());
            if position < visible.len() {
                visible[position] = worker;
                continue;
            }
        }
        visible.push(worker);
    }
    visible
}

/// Pet mood for a worker chip; a finished-but-unsuccessful outcome looks failed, not happy.
pub(super) fn worker_mood(chip: WorkerChip, outcome: Option<WorkerOutcome>) -> Mood {
    match (chip, outcome) {
        (_, Some(WorkerOutcome::Succeeded)) => Mood::Responded,
        (_, Some(_)) => Mood::Failed,
        (WorkerChip::Running, _) => Mood::Working,
        (WorkerChip::NeedsYou, _) => Mood::Waiting,
        (WorkerChip::Done, _) => Mood::Responded,
        (WorkerChip::Starting, _) => Mood::Idle,
    }
}

/// 菜单里的两行项：标题加一行说明；`color` 用于危险操作。
fn rich_item(
    title: &'static str,
    note: Option<&'static str>,
    color: Option<u32>,
    secondary: u32,
    on_click: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        let title = div()
            .text_size(px(13.))
            .when_some(color, |row, color| row.text_color(rgba(color)))
            .child(t(title));
        div()
            .flex()
            .flex_col()
            .py(px(2.))
            .child(title)
            .when_some(note, |col, note| {
                col.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgba(secondary))
                        .child(t(note)),
                )
            })
    })
    .on_click(move |_, window, cx| on_click(window, cx))
}

/// 胶囊菜单：标题行、定位、移除；执行中只有「停止并移除」（会关卡片），已结束的另有「移除并关闭卡片」。
fn worker_menu(
    menu: PopupMenu,
    app: &gpui::WeakEntity<MultisoulApp>,
    target: (&str, &str),
    title: String,
    colors: (u32, u32),
    (node, running): (Option<u64>, bool),
) -> PopupMenu {
    let (task_id, dispatch_id) = target;
    let (secondary, danger) = colors;
    let locate = {
        let app = app.clone();
        PopupMenuItem::new(t("staff.locate_worker"))
            .disabled(node.is_none())
            .on_click(move |_, window, cx| {
                if let Some(node) = node {
                    let _ = app.update(cx, |app, cx| app.locate_staff_worker(node, window, cx));
                }
            })
    };
    let remove = |label: &'static str, note: Option<&'static str>, close_card: bool| {
        let (app, task_id, dispatch_id) = (app.clone(), task_id.to_owned(), dispatch_id.to_owned());
        rich_item(
            label,
            note,
            close_card.then_some(danger),
            secondary,
            move |window, cx| {
                let request = RemoveRequest {
                    task_id: task_id.clone(),
                    dispatch_id: dispatch_id.clone(),
                    close_card,
                    confirmed: false,
                };
                let _ = app.update(cx, |app, cx| app.request_remove_worker(request, window, cx));
            },
        )
    };
    let menu = menu
        .min_w(px(250.))
        .item(PopupMenuItem::label(title))
        .item(locate)
        .item(PopupMenuItem::separator());
    if running {
        return menu.item(remove(
            "staff.worker_stop_remove",
            Some("staff.worker_stop_note"),
            true,
        ));
    }
    let menu = menu.item(remove(
        "staff.worker_remove",
        Some("staff.worker_remove_note"),
        false,
    ));
    if node.is_some() {
        menu.item(remove("staff.worker_remove_close", None, true))
    } else {
        menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use norma::protocol::WorkerPhase;

    /// 场景：同一张卡收到第二次派发后，顶部继续显示一张卡并更新状态。
    /// 数据构造：卡 7 有一轮成功分析和一轮运行中的修复，中间加入同名的卡 8。
    /// 执行过程：顶部读取最新任务快照，生成用于渲染的 Worker 列表。
    /// 预期结果：卡 7 原位展示修复轮，分析轮从顶部消失，同名的卡 8 仍单独出现。
    #[test]
    fn staff_worker_strip_shows_latest_dispatch_once_per_card() {
        let mut analysis = Worker::new("Bruno", "Claude", "分析", 7, "analysis");
        analysis.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
        let other = Worker::new("Bruno", "Claude", "其他工作", 8, "other");
        let mut repair = Worker::new("Bruno", "Claude", "修复", 7, "repair");
        repair.reused = true;
        repair.phase = WorkerPhase::Running;
        let workers = [analysis, other, repair];

        let visible = workers_for_strip(&workers);
        assert_eq!(visible.len(), 2, "顶部应只显示两张不同的卡");
        assert_eq!(
            visible[0].dispatch_id, "repair",
            "卡 7 应在原位置使用最新派发"
        );
        assert_eq!(
            visible[0].chip(),
            WorkerChip::Running,
            "卡 7 应显示最新派发的执行状态"
        );
        assert!(
            visible
                .iter()
                .all(|worker| worker.dispatch_id != "analysis"),
            "已完成的分析轮不应再占一张顶部卡片"
        );
        assert_eq!(visible[1].dispatch_id, "other", "同名的卡 8 应继续单独显示");
    }

    #[test]
    fn worker_mood_follows_chip_and_outcome() {
        assert_eq!(worker_mood(WorkerChip::Running, None), Mood::Working);
        assert_eq!(worker_mood(WorkerChip::NeedsYou, None), Mood::Waiting);
        assert_eq!(worker_mood(WorkerChip::Starting, None), Mood::Idle);
        assert_eq!(
            worker_mood(WorkerChip::Done, Some(WorkerOutcome::Succeeded)),
            Mood::Responded
        );
        assert_eq!(
            worker_mood(WorkerChip::Done, Some(WorkerOutcome::Interrupted)),
            Mood::Failed
        );
    }
}
