use super::{
    view::{outcome_label, status_label},
    *,
};
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{TaskDisplayStatus, WorkerChip};
use ui::component::{
    avatar::Avatar,
    button::{Button, ButtonVariants},
    Disableable, Sizable, StyledExt,
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
                    .disabled(node.is_none())
                    .tooltip(format!("{} · {}", worker.role, t("staff.locate_worker")))
                    .child(Avatar::new().name(worker.agent.clone()).with_size(px(24.)))
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
                    .on_click(cx.listener(move |app, _, window, cx| {
                        if let Some(node) = node {
                            app.locate_staff_worker(node, window, cx);
                        }
                    })),
            );
        }
        strip
    }
}
