use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Stateful, Window};
use norma::protocol::{TaskDisplayStatus, TaskPhase, WorkerOutcome};
use ui::component::{
    button::{Button, ButtonVariants},
    Sizable, StyledExt,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(crate) fn render_staff_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let layout = self.desktop_assistant_layout(window);
        self.sync_staff_workspace(window, cx);
        self.ensure_staff_inputs(window, cx);
        let theme = self.staff_theme();
        div()
            .id("staff-workspace")
            .debug_selector(|| "staff-workspace".into())
            .absolute()
            .top(px(self.staff_panel_top()))
            .bottom_0()
            .left_0()
            .right_0()
            .border_t_1()
            .border_color(rgba(self.staff_line()))
            .bg(rgba(theme.graphite))
            .text_color(rgba(theme.text_primary))
            .text_size(px(14.))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("staff-task-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .mr(px(layout.right_space))
                    .p(px(12.))
                    .gap(px(12.))
                    .when(
                        super::sidebar_width(window) > 0. && !self.staff.fullscreen,
                        |body| body.child(self.staff_task_list("", window, cx)),
                    )
                    .child(self.staff_conversation_area(window, cx)),
            )
            .when(
                super::sidebar_width(window) == 0. && self.staff.sidebar_open,
                |panel| {
                    panel.child(
                        div()
                            .absolute()
                            .top(px(8.))
                            .bottom(px(12.))
                            .left(px(8.))
                            .w(px(280.))
                            .rounded(px(16.))
                            .bg(rgba(theme.graphite))
                            .border_1()
                            .border_color(rgba(self.staff_line()))
                            .occlude()
                            .child(self.staff_task_list("", window, cx)),
                    )
                },
            )
    }

    pub(super) fn render_staff_conversation(
        &mut self,
        project_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.load_staff_tasks(cx);
        self.ensure_staff_inputs(window, cx);
        let selected = self
            .staff
            .selected
            .as_ref()
            .and_then(|id| self.staff.tasks.get(id))
            .cloned();
        let mut center = div().flex_1().min_w_0().h_full().flex().flex_col();
        if let Some(task) = &selected {
            center = center
                .child(self.staff_task_header(task, project_name, cx))
                .when(!task.workers.is_empty(), |center| {
                    center.child(self.staff_workers(task, cx))
                });
            let mut messages = div()
                .id("staff-messages")
                .track_scroll(&self.staff.scroll)
                .debug_selector(|| "staff-messages".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(28.))
                .py(px(16.))
                .flex()
                .flex_col()
                .gap(px(18.));
            for (index, message) in task.messages.iter().enumerate() {
                messages = messages.child(
                    div()
                        .w_full()
                        .max_w(px(760.))
                        .mx_auto()
                        .child(self.staff_message(task, index, message, window, cx)),
                );
            }
            if let Some(activity) = self.staff.activity.get(&task.id) {
                messages = messages.child(
                    div()
                        .w_full()
                        .max_w(px(760.))
                        .mx_auto()
                        .child(self.staff_activity_view(activity)),
                );
            }
            center = center.child(messages);
        } else {
            center = center.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .items_center()
                    .gap(px(14.))
                    .p(px(28.))
                    .child(crate::app::chrome::brand_logo())
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_semibold()
                            .child(t("staff.start_title")),
                    )
                    .child(
                        div()
                            .max_w(px(460.))
                            .text_size(px(14.))
                            .text_center()
                            .text_color(rgba(self.staff_theme().text_secondary))
                            .child(t("staff.start_hint")),
                    ),
            );
        }
        if let Some(error) = &self.staff.error {
            center = center.child(
                div()
                    .px(px(28.))
                    .py_2()
                    .text_size(px(12.))
                    .text_color(rgba(self.staff_theme().destructive))
                    .child(error.clone())
                    .child(
                        Button::new("staff-retry")
                            .ghost()
                            .small()
                            .label(t("staff.retry"))
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.staff.error = None;
                                app.staff.loaded = false;
                                app.load_staff_tasks(cx);
                                app.notify_staff(cx);
                            })),
                    ),
            );
        }
        center = center.child(self.staff_composer(selected.as_ref(), project_name, window, cx));
        center
    }

    pub(super) fn staff_line(&self) -> u32 {
        ui::icons::with_alpha(self.staff_theme().text_primary, 22)
    }
}

pub(super) fn status_label(status: TaskDisplayStatus) -> &'static str {
    t(match status {
        TaskDisplayStatus::NeedsYou => "staff.needs_you",
        TaskDisplayStatus::Running => "staff.running",
        TaskDisplayStatus::Waiting => "staff.waiting",
        TaskDisplayStatus::Merging => "staff.merging",
        TaskDisplayStatus::Done => "staff.done",
    })
}
pub(super) fn phase_label(phase: TaskPhase) -> &'static str {
    t(match phase {
        TaskPhase::Active => "staff.active",
        TaskPhase::Merging => "staff.merging",
        TaskPhase::Done => "staff.done",
    })
}
pub(super) fn outcome_label(outcome: WorkerOutcome) -> &'static str {
    t(match outcome {
        WorkerOutcome::Succeeded => "staff.result_received",
        WorkerOutcome::Failed => "staff.failed",
        WorkerOutcome::Interrupted => "staff.interrupted",
        WorkerOutcome::Unavailable => "staff.result_unavailable",
    })
}
