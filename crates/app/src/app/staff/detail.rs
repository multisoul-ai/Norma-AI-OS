use super::{
    task_list::status_color,
    view::{outcome_label, phase_label, status_label},
    *,
};
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{StaffOp, TaskDisplayStatus, TaskPhase, WorkerOutcome, WorkerState};
use ui::component::{
    avatar::Avatar,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, IconName, Sizable, StyledExt,
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
            .outline()
            .compact()
            .h(px(24.))
            .text_size(px(12.))
            .label(phase_label(current))
            .disabled(self.staff.busy)
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
        let color = status_color(task.display_status(), self.theme);
        let first_node = task.workers.iter().find_map(|worker| worker.card_id);
        let meta = ui::i18n::format(
            "staff.task_meta",
            &[
                ("project", project_name.to_owned()),
                ("count", task.workers.len().to_string()),
            ],
        );
        div()
            .px(px(28.))
            .pt(px(20.))
            .pb(px(14.))
            .flex_none()
            .border_b_1()
            .border_color(rgba(self.staff_line()))
            .child(
                div()
                    .max_w(px(760.))
                    .mx_auto()
                    .flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded(px(5.))
                                    .text_size(px(11.))
                                    .text_color(rgba(color))
                                    .bg(rgba(ui::icons::with_alpha(color, 40)))
                                    .child(status_label(task.display_status())),
                            )
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_semibold()
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(12.))
                                    .text_color(rgba(self.theme.text_secondary))
                                    .child(meta)
                                    .child(t("staff.phase"))
                                    .child(menu),
                            )
                            .when(task.phase_manual, |header| {
                                header.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgba(self.theme.text_secondary))
                                        .child(t("staff.manual_phase")),
                                )
                            }),
                    )
                    .when_some(first_node, |header, node| {
                        header.child(
                            Button::new("staff-locate-task")
                                .outline()
                                .compact()
                                .h(px(34.))
                                .rounded(px(17.))
                                .icon(IconName::ExternalLink)
                                .tooltip(t("staff.locate_worker"))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.locate_staff_worker(node, window, cx)
                                })),
                        )
                    }),
            )
    }

    pub(super) fn staff_workers(&self, task: &Task, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let mut strip = div()
            .id("staff-workers")
            .debug_selector(|| "staff-workers".into())
            .flex_none()
            .flex()
            .gap(px(8.))
            .overflow_x_scroll()
            .px(px(28.))
            .py(px(12.))
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
                    .outline()
                    .w(px(230.))
                    .min_w(px(230.))
                    .h_auto()
                    .p(px(12.))
                    .rounded(px(11.))
                    .flex_none()
                    .justify_start()
                    .disabled(node.is_none())
                    .tooltip(t("staff.locate_worker"))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(7.))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        Avatar::new().name(worker.agent.clone()).with_size(px(26.)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_medium()
                                            .child(worker.agent.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgba(theme.text_tertiary))
                                            .child(worker.engine.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .text_ellipsis()
                                    .text_size(px(12.))
                                    .child(worker.role.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .text_size(px(11.))
                                    .text_color(rgba(color))
                                    .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(rgba(color)))
                                    .child(status_text),
                            ),
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
