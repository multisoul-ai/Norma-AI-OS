use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Stateful, Window};
use norma::protocol::{TaskDisplayStatus, TaskPhase, WorkerOutcome};
use ui::component::{
    button::{Button, ButtonVariants},
    IconName, Selectable, Sizable, StyledExt,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(crate) fn render_staff_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let layout = self.desktop_assistant_layout(window);
        let project = self.expanded_desktop_project();
        let workspace = self
            .desktop_workspace
            .active()
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let project_name = match self.desktop_context_for_provider(cx) {
            Some(agent::DesktopContext::ProjectCanvas { project_name, .. }) => project_name,
            _ => String::new(),
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(22.))
            .h(px(56.))
            .flex_none()
            .when(self.staff.fullscreen, |header| {
                header.pl(px(crate::app::chrome::BRAND_TOGGLE_LEFT))
            })
            .border_b_1()
            .border_color(rgba(self.staff_line()))
            .child(crate::app::chrome::brand_logo())
            .child(div().text_size(px(17.)).font_semibold().child("Norma"))
            .child(
                div()
                    .pl(px(12.))
                    .border_l_1()
                    .border_color(rgba(self.staff_line()))
                    .text_size(px(13.))
                    .text_color(rgba(self.theme.text_secondary))
                    .child(t("staff.mode")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_right()
                    .text_size(px(12.))
                    .text_color(rgba(self.theme.text_secondary))
                    .child(format!("{workspace}  /  {project_name}")),
            )
            .child(
                Button::new("staff-fullscreen")
                    .debug_selector(|| "staff-fullscreen".into())
                    .ghost()
                    .with_size(px(30.))
                    .icon(if self.staff.fullscreen {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .tooltip(if self.staff.fullscreen {
                        t("staff.exit_fullscreen")
                    } else {
                        t("staff.fullscreen")
                    })
                    .selected(self.staff.fullscreen)
                    .on_click(
                        cx.listener(|app, _, window, cx| app.toggle_staff_fullscreen(window, cx)),
                    ),
            )
            .child(
                Button::new("staff-back-canvas")
                    .ghost()
                    .with_size(px(30.))
                    .icon(IconName::Close)
                    .tooltip(t("staff.canvas"))
                    .on_click(
                        cx.listener(|app, _, window, cx| app.set_staff_mode(false, window, cx)),
                    ),
            );
        div()
            .id("staff-workspace")
            .debug_selector(|| "staff-workspace".into())
            .absolute()
            .top(px(if self.staff.fullscreen {
                0.
            } else {
                crate::app::chrome::TOP_BAR_HEIGHT
            }))
            .bottom_0()
            .left_0()
            .right(px(layout.right_space))
            .bg(rgba(self.theme.graphite))
            .text_color(rgba(self.theme.text_primary))
            .text_size(px(14.))
            .flex()
            .flex_col()
            .child(header)
            .when_some(project.clone(), |panel, project| {
                panel.child(project.update(cx, |app, cx| {
                    app.render_staff_tasks(&project_name, window, cx)
                }))
            })
            .when(project.is_none(), |panel| {
                panel.child(div().p_6().child(t("staff.choose_project")))
            })
    }

    fn render_staff_tasks(
        &mut self,
        project_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.load_staff_tasks(cx);
        self.ensure_staff_inputs(window, cx);
        let selected = self
            .staff
            .selected
            .as_ref()
            .and_then(|id| self.staff.tasks.get(id))
            .cloned();
        let list = self.staff_task_list(project_name, window, cx);
        let mut center = div().flex_1().min_w_0().h_full().flex().flex_col();
        if let Some(task) = &selected {
            center = center
                .child(self.staff_task_header(task, project_name, cx))
                .child(self.staff_workers(task, cx));
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
                        .child(self.staff_message(task, index, message, cx)),
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
                            .text_color(rgba(self.theme.text_secondary))
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
                    .text_color(rgba(self.theme.destructive))
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
        center = center.child(self.staff_composer(selected.as_ref(), project_name, cx));
        let need = self
            .staff
            .tasks
            .values()
            .filter(|task| task.display_status() == TaskDisplayStatus::NeedsYou)
            .count();
        let run = self
            .staff
            .tasks
            .values()
            .filter(|task| task.display_status() == TaskDisplayStatus::Running)
            .count();
        let footer = div()
            .h(px(34.))
            .flex_none()
            .px(px(18.))
            .flex()
            .items_center()
            .gap(px(12.))
            .border_t_1()
            .border_color(rgba(self.staff_line()))
            .text_size(px(11.))
            .text_color(rgba(self.theme.text_secondary))
            .child(format!("{} {need}", t("staff.needs_you")))
            .child("·")
            .child(format!("{} {run}", t("staff.running")))
            .child(div().flex_1())
            .child(t("staff.footer_hint"));
        div()
            .id("staff-task-body")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(super::sidebar_width(window) > 0.0, |body| body.child(list))
                    .child(center),
            )
            .child(footer)
    }

    pub(super) fn staff_line(&self) -> u32 {
        ui::icons::with_alpha(self.theme.text_primary, 22)
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
