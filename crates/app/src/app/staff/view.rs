use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Stateful, Window};
use norma::protocol::{StaffOp, TaskDisplayStatus, TaskPhase, WorkerOutcome};
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, Selectable, Sizable, StyledExt,
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
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .when(self.staff.fullscreen, |header| {
                header.pl(px(crate::app::chrome::BRAND_TOGGLE_LEFT))
            })
            .h(px(44.0))
            .border_b_1()
            .border_color(rgba(self.theme.node_stroke))
            .child(div().font_semibold().child(t("staff.tasks")))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("staff-back-canvas")
                            .ghost()
                            .small()
                            .label(t("staff.canvas"))
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.set_staff_mode(false, window, cx)
                            })),
                    )
                    .child(
                        Button::new("staff-fullscreen")
                            .debug_selector(|| "staff-fullscreen".into())
                            .ghost()
                            .small()
                            .label(if self.staff.fullscreen {
                                t("staff.exit_fullscreen")
                            } else {
                                t("staff.fullscreen")
                            })
                            .selected(self.staff.fullscreen)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.toggle_staff_fullscreen(window, cx)
                            })),
                    ),
            );
        div()
            .id("staff-workspace")
            .debug_selector(|| "staff-workspace".into())
            .absolute()
            .top(px(if self.staff.fullscreen { 0.0 } else { 52.0 }))
            .bottom_0()
            .left_0()
            .right(px(layout.right_space))
            .bg(rgba(self.theme.graphite))
            .text_color(rgba(self.theme.text_primary))
            .flex()
            .flex_col()
            .child(header)
            .when_some(project.clone(), |panel, project| {
                panel.child(project.update(cx, |app, cx| app.render_staff_tasks(window, cx)))
            })
            .when(project.is_none(), |panel| {
                panel.child(div().p_6().child(t("staff.choose_project")))
            })
    }

    fn render_staff_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        self.load_staff_tasks(cx);
        self.ensure_staff_input(window, cx);
        let theme = self.theme;
        let selected = self
            .staff
            .selected
            .as_ref()
            .and_then(|id| self.staff.tasks.get(id))
            .cloned();
        let mut list = div()
            .id("staff-task-list")
            .debug_selector(|| "staff-task-list".into())
            .w(px(232.0))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .border_r_1()
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(theme.panel))
            .child(
                Button::new("staff-new-task")
                    .debug_selector(|| "staff-new-task".into())
                    .outline()
                    .w_full()
                    .label(t("staff.new_task"))
                    .disabled(self.staff.busy)
                    .on_click(
                        cx.listener(|app, _, window, cx| app.select_staff_task(None, window, cx)),
                    ),
            );
        for status in [
            TaskDisplayStatus::NeedsYou,
            TaskDisplayStatus::Running,
            TaskDisplayStatus::Waiting,
            TaskDisplayStatus::Merging,
            TaskDisplayStatus::Done,
        ] {
            let tasks: Vec<_> = self
                .staff
                .tasks
                .values()
                .filter(|task| task.display_status() == status)
                .cloned()
                .collect();
            if tasks.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .mt_4()
                    .mb_2()
                    .text_xs()
                    .text_color(rgba(theme.text_secondary))
                    .child(format!("{} · {}", status_label(status), tasks.len())),
            );
            for task in tasks {
                let id = task.id.clone();
                list = list.child(
                    Button::new(gpui::SharedString::from(format!("staff-task-{}", task.id)))
                        .debug_selector(move || format!("staff-task-{}", task.id))
                        .ghost()
                        .w_full()
                        .h_auto()
                        .py_2()
                        .justify_start()
                        .selected(self.staff.selected.as_deref() == Some(&id))
                        .disabled(self.staff.busy)
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(task.title.clone()),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.select_staff_task(Some(id.clone()), window, cx)
                        })),
                );
            }
        }
        let mut center = div().flex_1().min_w_0().h_full().flex().flex_col();
        if let Some(task) = selected {
            center = center.child(self.staff_task_header(&task, cx));
            let mut messages = div()
                .id("staff-messages")
                .track_scroll(&self.staff.scroll)
                .debug_selector(|| "staff-messages".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_6()
                .py_4()
                .flex()
                .flex_col()
                .gap_4();
            for (index, message) in task.messages.iter().enumerate() {
                messages = messages.child(self.staff_message(&task, index, message, cx));
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
                    .gap_3()
                    .p_6()
                    .child(
                        div()
                            .text_xl()
                            .font_semibold()
                            .child(t("staff.start_title")),
                    )
                    .child(
                        div()
                            .text_color(rgba(theme.text_secondary))
                            .child(t("staff.start_hint")),
                    ),
            );
        }
        if let Some(error) = &self.staff.error {
            center = center.child(
                div()
                    .px_4()
                    .py_2()
                    .text_color(rgba(theme.destructive))
                    .child(error.clone())
                    .child(
                        Button::new("staff-retry")
                            .ghost()
                            .small()
                            .label(t("staff.retry"))
                            .disabled(self.staff.busy)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.staff.error = None;
                                app.staff.loaded = false;
                                app.load_staff_tasks(cx);
                                app.notify_staff(cx);
                            })),
                    ),
            );
        }
        let input = self
            .staff
            .input
            .as_ref()
            .expect("staff input initialized")
            .clone();
        center = center.child(
            div()
                .border_t_1()
                .border_color(rgba(theme.node_stroke))
                .p_4()
                .flex()
                .items_end()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&input).disabled(self.staff.busy)),
                )
                .child(
                    Button::new("staff-send")
                        .debug_selector(|| "staff-send".into())
                        .primary()
                        .label(t("staff.send"))
                        .disabled(self.staff.busy || input.read(cx).value().trim().is_empty())
                        .on_click(cx.listener(Self::send_staff_message_click)),
                ),
        );
        div()
            .id("staff-task-body")
            .flex_1()
            .min_h_0()
            .flex()
            .child(list)
            .child(center)
    }

    fn staff_task_header(&mut self, task: &Task, cx: &mut Context<Self>) -> Div {
        let current = task.phase;
        let id = task.id.clone();
        let entity = cx.weak_entity();
        let menu = Button::new("staff-phase")
            .debug_selector(|| "staff-phase".into())
            .outline()
            .small()
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
        let mut workers = div().flex().flex_wrap().gap_2();
        for worker in &task.workers {
            if let Some(node) = worker.card_id {
                let label = format!(
                    "{} · {}",
                    worker.agent,
                    worker
                        .outcome
                        .map(outcome_label)
                        .unwrap_or(t("staff.running"))
                );
                workers = workers.child(
                    Button::new(gpui::SharedString::from(format!("staff-worker-{node}")))
                        .ghost()
                        .small()
                        .label(label)
                        .tooltip(t("staff.locate_worker"))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.locate_staff_worker(node, window, cx)
                        })),
                );
            }
        }
        div()
            .px_6()
            .py_4()
            .border_b_1()
            .border_color(rgba(self.theme.node_stroke))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_lg()
                            .font_semibold()
                            .child(task.title.clone()),
                    )
                    .child(menu),
            )
            .when(task.phase_manual, |header| {
                header.child(
                    div()
                        .text_xs()
                        .text_color(rgba(self.theme.text_secondary))
                        .child(t("staff.manual_phase")),
                )
            })
            .child(workers)
    }

    fn ensure_staff_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.staff.input.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(ui::i18n::input_placeholder(
                    "staff.message_placeholder",
                    window,
                    cx,
                ))
                // Match the command dock: Enter sends; single-line layout rejects newlines.
                .validate(|value, _| !value.contains(['\n', '\r']))
        });
        cx.subscribe_in(&input, window, |app, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => app.send_staff_message(window, cx),
            InputEvent::Change => app.notify_staff(cx),
            _ => {}
        })
        .detach();
        self.staff.input = Some(input);
    }
}

fn status_label(status: TaskDisplayStatus) -> &'static str {
    t(match status {
        TaskDisplayStatus::NeedsYou => "staff.needs_you",
        TaskDisplayStatus::Running => "staff.running",
        TaskDisplayStatus::Waiting => "staff.waiting",
        TaskDisplayStatus::Merging => "staff.merging",
        TaskDisplayStatus::Done => "staff.done",
    })
}
fn phase_label(phase: TaskPhase) -> &'static str {
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
