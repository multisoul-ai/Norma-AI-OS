use super::{
    task_list::status_color,
    view::{outcome_label, phase_label, status_label},
    *,
};
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{StaffOp, TaskDisplayStatus, TaskPhase, WorkerOutcome, WorkerState};
use ui::component::{
    avatar::Avatar,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
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
        let current = task.phase;
        let id = task.id.clone();
        let entity = cx.weak_entity();
        let menu = Button::new("staff-phase")
            .debug_selector(|| "staff-phase".into())
            .custom(self.staff_button_style(false, cx))
            .compact()
            .h(px(32.))
            .rounded(px(16.))
            .text_size(px(12.))
            .child(div().text_size(px(12.)).child(phase_label(current)))
            .disabled(self.staff.busy)
            .tooltip(if task.phase_manual {
                t("staff.manual_phase")
            } else {
                t("staff.phase")
            })
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for phase in [TaskPhase::Active, TaskPhase::Merging, TaskPhase::Done] {
                    let entity = entity.clone();
                    let id = id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(phase_label(phase))
                            .checked(current == phase)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |app, cx| {
                                    app.staff_user_operation(
                                        StaffOp::Phase {
                                            task_id: id.clone(),
                                            phase,
                                        },
                                        true,
                                        cx,
                                    )
                                });
                            }),
                    );
                }
                menu
            });
        div()
            .px(px(28.))
            .py(px(22.))
            .flex_none()
            .border_b_1()
            .border_color(rgba(self.staff_line()))
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
                            .child(project_name.to_owned())
                            .when(task.phase_manual, |meta| {
                                meta.child(format!(" · {}", t("staff.manual_phase")))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(21.))
                            .font_semibold()
                            .child(task.title.clone()),
                    ),
            )
            .child(div().flex_none().child(menu))
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
            .px(px(28.))
            .py(px(8.))
            .border_b_1()
            .border_color(rgba(self.staff_line()));
        if task.workers.is_empty() {
            return strip.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba(theme.text_tertiary))
                    .child(t("staff.no_workers")),
            );
        }
        for (index, worker) in task.workers.iter().enumerate() {
            let status = if worker.needs_input
                || matches!(worker.state, WorkerState::Failed | WorkerState::Interrupted)
                || worker.outcome == Some(WorkerOutcome::Unavailable)
            {
                TaskDisplayStatus::NeedsYou
            } else if worker.outcome.is_some() {
                TaskDisplayStatus::Done
            } else {
                TaskDisplayStatus::Running
            };
            let color = status_color(status, theme);
            let status_text = worker
                .outcome
                .map(outcome_label)
                .unwrap_or_else(|| status_label(status));
            let node = worker.card_id;
            strip = strip.child(
                Button::new(("staff-worker", index))
                    .debug_selector(move || format!("staff-worker-{}", node.unwrap_or_default()))
                    .custom(self.staff_button_style(false, cx))
                    .h(px(38.))
                    .px(px(10.))
                    .rounded(px(12.))
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
