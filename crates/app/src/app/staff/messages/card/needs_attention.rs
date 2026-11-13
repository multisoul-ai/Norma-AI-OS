use super::super::*;
use super::shell;
use norma::protocol::WorkerState;
use ui::component::Disableable;
use ui::i18n::text as t;

pub(in crate::app::staff::messages) struct NeedsAttention<'a>(pub &'a TaskMessage);

impl MessageBlock for NeedsAttention<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let index = ctx.index;
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::WorkerNeedsAttention {
            node_id,
            dispatch_id,
        } = message
        {
            let node = *node_id;
            let dispatch = dispatch_id.clone();
            let can_redeliver = task.workers.iter().any(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == *dispatch_id
                    && worker.state == WorkerState::NeedsAttention
            });
            row = shell::card(ctx, false)
                .child(
                    div()
                        .text_size(px(tokens::LABEL_TEXT))
                        .child(t("staff.worker_needs_attention")),
                )
                .child(
                    shell::header()
                        .child(
                            shell::action_button(ctx, ("staff-redeliver", index), true, cx)
                                .debug_selector(|| "staff-redeliver".into())
                                .label(t("staff.redeliver"))
                                .disabled(ctx.app.staff.busy || !can_redeliver)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.redeliver_staff_worker(node, &dispatch, cx);
                                })),
                        )
                        .child(
                            shell::action_button(ctx, ("staff-view-worker", index), false, cx)
                                .debug_selector(|| "staff-view-worker".into())
                                .label(t("staff.locate_worker"))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.locate_staff_worker(node, window, cx);
                                })),
                        ),
                );
        }
        row.into_any_element()
    }
}
