use super::view::outcome_label;
use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Stateful};
use norma::protocol::{TaskMessage, WorkerOutcome};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Sizable, StyledExt,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn staff_message(
        &self,
        task: &Task,
        index: usize,
        message: &TaskMessage,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let mut row = div()
            .id(("staff-message", index))
            .w_full()
            .flex()
            .flex_col()
            .gap_2();
        match message {
            TaskMessage::User { text } | TaskMessage::Norma { text } => {
                let user = matches!(message, TaskMessage::User { .. });
                row = row
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(rgba(theme.text_secondary))
                            .child(if user { t("staff.you") } else { "Norma" }),
                    )
                    .child(
                        div()
                            .p_3()
                            .rounded(px(10.0))
                            .bg(rgba(if user { theme.panel_high } else { theme.panel }))
                            .child(text.clone()),
                    );
            }
            TaskMessage::Proposal { proposal_id } => {
                if let Some(proposal) = task.proposals.iter().find(|p| p.id == *proposal_id) {
                    let id = task.id.clone();
                    let proposal_id = *proposal_id;
                    row = row
                        .border_1()
                        .border_color(rgba(theme.node_stroke))
                        .rounded(px(12.0))
                        .p_4()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::format(
                                    "staff.proposal_for",
                                    &[("agent", proposal.agent.clone())],
                                )),
                        )
                        .child(div().font_semibold().child(proposal.title.clone()))
                        .child(proposal.why.clone())
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgba(theme.text_secondary))
                                .child(proposal.prompt.clone()),
                        )
                        .child(
                            Button::new(("staff-adopt", proposal_id))
                                .debug_selector(move || format!("staff-adopt-{proposal_id}"))
                                .primary()
                                .small()
                                .label(if proposal.dispatch_id.is_some() {
                                    t("staff.dispatched")
                                } else {
                                    t("staff.adopt")
                                })
                                .disabled(self.staff.busy || proposal.dispatch_id.is_some())
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.approve_staff_proposal(id.clone(), proposal_id, cx)
                                })),
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
                    .border_l_2()
                    .border_color(rgba(if *outcome == WorkerOutcome::Succeeded {
                        theme.success
                    } else {
                        theme.accent_orange
                    }))
                    .pl_3()
                    .child(
                        div()
                            .font_semibold()
                            .child(format!("{name} · {}", outcome_label(*outcome))),
                    )
                    .when(*outcome != WorkerOutcome::Unavailable, |row| {
                        row.child(
                            if matches!(
                                text.as_str(),
                                "staff.worker_failed" | "staff.worker_not_ready"
                            ) {
                                error_message(text.clone()).to_string()
                            } else {
                                text.clone()
                            },
                        )
                    });
            }
            TaskMessage::WorkerStarted { .. } => {
                row = row
                    .text_xs()
                    .text_color(rgba(theme.text_secondary))
                    .child(t("staff.worker_started"))
            }
            TaskMessage::Reopened => {
                row = row
                    .text_xs()
                    .text_color(rgba(theme.text_secondary))
                    .child(t("staff.reopened"))
            }
        }
        row
    }
}
