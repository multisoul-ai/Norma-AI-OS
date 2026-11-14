use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::WorkerOutcome;
use ui::component::{avatar::Avatar, Sizable};

pub(in crate::app::staff::messages) struct WorkerResult<'a>(pub &'a TaskMessage);

impl MessageBlock for WorkerResult<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        _cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::WorkerFinished {
            node_id,
            outcome,
            text,
            ..
        } = message
        {
            let name = task
                .workers
                .iter()
                .find(|worker| worker.card_id == Some(*node_id))
                .map(|worker| worker.agent.as_str())
                .unwrap_or("Agent");
            row = shell::card(ctx, false)
                .child(
                    shell::header()
                        .text_size(px(tokens::SPACE_12))
                        .child(
                            Avatar::new()
                                .name(name.to_owned())
                                .with_size(px(tokens::AVATAR_SIZE)),
                        )
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
                    row.child(div().text_size(px(tokens::LABEL_TEXT)).child(
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
        row.into_any_element()
    }
}
