use super::worker_remove::RemoveRequest;
use super::{
    view::{outcome_label, status_label},
    *,
};
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{TaskDisplayStatus, WorkerChip, WorkerOutcome};
use runtime::island::bloub::Mood;
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
        for (index, worker) in task.workers.iter().enumerate() {
            let chip = worker.chip();
            let color = match chip {
                WorkerChip::NeedsYou => theme.accent_warm,
                WorkerChip::Running => theme.accent_cyan,
                WorkerChip::Starting => theme.accent_violet,
                WorkerChip::Done => theme.success,
            };
            let status_text = worker.outcome.map(outcome_label).unwrap_or(match chip {
                WorkerChip::Starting => t("staff.worker_starting"),
                WorkerChip::Running => status_label(TaskDisplayStatus::Running),
                WorkerChip::NeedsYou => status_label(TaskDisplayStatus::NeedsYou),
                WorkerChip::Done => status_label(TaskDisplayStatus::Done),
            });
            let node = worker.card_id;
            let (app, task_id, dispatch_id) = (
                cx.weak_entity(),
                task.id.clone(),
                worker.dispatch_id.clone(),
            );
            let running = worker.outcome.is_none();
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
                        worker_mood(chip, worker.outcome),
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
                    .dropdown_menu(move |menu, _, _| {
                        worker_menu(menu, &app, &task_id, &dispatch_id, node, running)
                    }),
            );
        }
        strip
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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

/// 胶囊菜单：定位、移除；执行中只有「停止并移除」（会关卡片），已结束的另有「移除并关闭卡片」。
fn worker_menu(
    menu: PopupMenu,
    app: &gpui::WeakEntity<MultisoulApp>,
    task_id: &str,
    dispatch_id: &str,
    node: Option<u64>,
    running: bool,
) -> PopupMenu {
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
    let remove = |label: &'static str, close_card: bool| {
        let (app, task_id, dispatch_id) = (app.clone(), task_id.to_owned(), dispatch_id.to_owned());
        PopupMenuItem::new(t(label)).on_click(move |_, window, cx| {
            let request = RemoveRequest {
                task_id: task_id.clone(),
                dispatch_id: dispatch_id.clone(),
                close_card,
                confirmed: false,
            };
            let _ = app.update(cx, |app, cx| app.request_remove_worker(request, window, cx));
        })
    };
    let menu = menu.item(locate).item(PopupMenuItem::separator());
    if running {
        return menu.item(remove("staff.worker_stop_remove", true));
    }
    let menu = menu.item(remove("staff.worker_remove", false));
    if node.is_some() {
        menu.item(remove("staff.worker_remove_close", true))
    } else {
        menu
    }
}
