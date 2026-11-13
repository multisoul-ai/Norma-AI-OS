use super::view::outcome_label;
use super::*;
use gpui::{div, prelude::*, px, relative, rgba, Div, Stateful, Window};
use norma::protocol::{TaskMessage, WorkerOutcome, WorkerState};
use ui::component::{
    avatar::Avatar,
    button::{Button, ButtonVariants},
    text::TextView,
    Disableable, IconName, Sizable, StyledExt,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn staff_message(
        &self,
        task: &Task,
        index: usize,
        message: &TaskMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.staff_theme();
        let mut row = div()
            .id(("staff-message", index))
            .debug_selector(move || format!("staff-message-body-{index}"))
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(14.))
            .line_height(relative(1.7));
        match message {
            TaskMessage::User { text } => {
                row = row.flex_row().justify_end().child(
                    div()
                        .debug_selector(move || format!("staff-user-bubble-{index}"))
                        .max_w(relative(0.78))
                        .px(px(16.))
                        .py(px(11.))
                        .rounded(px(18.))
                        .rounded_br(px(4.))
                        .bg(rgba(theme.panel_high))
                        .child(text.clone()),
                );
            }
            TaskMessage::Norma { text } => {
                row = row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(13.))
                            .font_semibold()
                            .child(crate::app::chrome::brand_logo())
                            .child("Norma")
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_normal()
                                    .text_color(rgba(theme.text_secondary))
                                    .child(t("staff.coordination")),
                            ),
                    )
                    .child(
                        TextView::markdown(
                            gpui::SharedString::from(format!("staff-markdown-{}-{index}", task.id)),
                            text.clone(),
                            window,
                            cx,
                        )
                        .selectable(true),
                    );
            }
            TaskMessage::Proposal { proposal_id } => {
                if let Some(proposal) = task.proposals.iter().find(|p| p.id == *proposal_id) {
                    let id = task.id.clone();
                    let proposal_id = *proposal_id;
                    let draft = proposal.prompt.clone();
                    let dispatched = proposal.dispatch_id.is_some();
                    // Material filled card: tonal surface, no outline.
                    row = row
                        .gap(px(12.))
                        .rounded(px(20.))
                        .px(px(20.))
                        .py(px(18.))
                        .bg(rgba(theme.panel_high))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(9.))
                                .text_size(px(13.))
                                .font_semibold()
                                .child(crate::app::chrome::brand_logo())
                                .child(t("staff.suggestion"))
                                .child(div().flex_1())
                                .when(!dispatched, |row| {
                                    row.child(
                                        div()
                                            .text_size(px(11.))
                                            .font_normal()
                                            .text_color(rgba(theme.accent_warm))
                                            .child(t("staff.your_decision")),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_semibold()
                                .child(proposal.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgba(theme.text_secondary))
                                .child(proposal.why.clone()),
                        )
                        .child(
                            div()
                                .px(px(14.))
                                .py(px(12.))
                                .rounded(px(14.))
                                .bg(rgba(theme.panel))
                                .text_size(px(13.))
                                .child(proposal.prompt.clone()),
                        )
                        .child(
                            div().flex().child(
                                div()
                                    .px(px(10.))
                                    .py(px(3.))
                                    .rounded_full()
                                    .bg(rgba(ui::icons::with_alpha(theme.accent_cyan, 30)))
                                    .text_size(px(12.))
                                    .text_color(rgba(theme.accent_cyan))
                                    .child(ui::i18n::format(
                                        "staff.proposal_for",
                                        &[("agent", proposal.agent.clone())],
                                    )),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_wrap()
                                .gap(px(8.))
                                .mt(px(4.))
                                .child(
                                    Button::new(("staff-adopt", proposal_id))
                                        .debug_selector(move || {
                                            format!("staff-adopt-{proposal_id}")
                                        })
                                        .custom(self.staff_primary_style(cx))
                                        .small()
                                        .compact()
                                        .h(px(36.))
                                        .px(px(18.))
                                        .rounded_full()
                                        .icon(IconName::Check)
                                        .label(if dispatched {
                                            t("staff.dispatched")
                                        } else {
                                            t("staff.adopt")
                                        })
                                        .disabled(self.staff.busy || dispatched)
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            app.approve_staff_proposal(id.clone(), proposal_id, cx)
                                        })),
                                )
                                .when(!dispatched, |actions| {
                                    actions.child(
                                        Button::new(("staff-edit-proposal", proposal_id))
                                            .custom(self.staff_button_style(false, cx))
                                            .compact()
                                            .h(px(36.))
                                            .px(px(16.))
                                            .rounded_full()
                                            .text_size(px(13.))
                                            .label(t("staff.edit_proposal"))
                                            .disabled(self.staff.busy)
                                            .on_click(cx.listener(move |app, _, window, cx| {
                                                app.edit_staff_proposal(&draft, window, cx);
                                            })),
                                    )
                                }),
                        );
                }
            }
            TaskMessage::WorkerFinished {
                node_id,
                outcome,
                text,
                ..
            } => {
                let name = task
                    .workers
                    .iter()
                    .find(|worker| worker.card_id == Some(*node_id))
                    .map(|worker| worker.agent.as_str())
                    .unwrap_or("Agent");
                row = row
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(16.))
                    .bg(rgba(theme.panel_high))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(Avatar::new().name(name.to_owned()).with_size(px(22.)))
                            .child(name.to_owned())
                            .child(
                                div()
                                    .text_color(rgba(if *outcome == WorkerOutcome::Succeeded {
                                        theme.success
                                    } else {
                                        theme.accent_warm
                                    }))
                                    .child(outcome_label(*outcome)),
                            ),
                    )
                    .when(*outcome != WorkerOutcome::Unavailable, |row| {
                        row.child(div().text_size(px(13.)).child(
                            if matches!(
                                text.as_str(),
                                "staff.worker_failed"
                                    | "staff.worker_not_ready"
                                    | "staff.worker_needs_attention"
                            ) {
                                error_message(text.clone()).to_string()
                            } else {
                                text.clone()
                            },
                        ))
                    });
            }
            TaskMessage::WorkerNeedsAttention {
                node_id,
                dispatch_id,
            } => {
                let node = *node_id;
                let dispatch = dispatch_id.clone();
                let can_redeliver = task.workers.iter().any(|worker| {
                    worker.card_id == Some(node)
                        && worker.dispatch_id == *dispatch_id
                        && worker.state == WorkerState::NeedsAttention
                });
                row = row
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(16.))
                    .bg(rgba(theme.panel_high))
                    .child(
                        div()
                            .text_size(px(13.))
                            .child(t("staff.worker_needs_attention")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                Button::new(("staff-redeliver", index))
                                    .debug_selector(|| "staff-redeliver".into())
                                    .custom(self.staff_primary_style(cx))
                                    .small()
                                    .compact()
                                    .h(px(36.))
                                    .px(px(18.))
                                    .rounded_full()
                                    .label(t("staff.redeliver"))
                                    .disabled(self.staff.busy || !can_redeliver)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.redeliver_staff_worker(node, &dispatch, cx);
                                    })),
                            )
                            .child(
                                Button::new(("staff-view-worker", index))
                                    .debug_selector(|| "staff-view-worker".into())
                                    .custom(self.staff_button_style(false, cx))
                                    .small()
                                    .compact()
                                    .h(px(36.))
                                    .px(px(16.))
                                    .rounded_full()
                                    .label(t("staff.locate_worker"))
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        app.locate_staff_worker(node, window, cx);
                                    })),
                            ),
                    );
            }
            TaskMessage::WorkerStarted { .. } | TaskMessage::Reopened => {
                row = row
                    .px(px(16.))
                    .py(px(10.))
                    .rounded(px(16.))
                    .bg(rgba(ui::icons::with_alpha(theme.panel_high, 140)))
                    .text_size(px(12.))
                    .text_color(rgba(theme.text_secondary))
                    .child(if matches!(message, TaskMessage::Reopened) {
                        t("staff.reopened")
                    } else {
                        t("staff.worker_started")
                    });
            }
        }
        row
    }
}
