//! 回合之间的一行事件条：Worker 启动、未送达、任务重开。已解决的事件变灰、去掉按钮。
use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::{WorkerFailure, WorkerPhase};
use ui::component::Disableable;
use ui::i18n::text as t;

pub(in crate::app::staff::messages) struct Event<'a>(pub &'a TaskMessage);

impl MessageBlock for Event<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let theme = ctx.app.staff_theme();
        let index = ctx.index;
        // 未解决的“未送达”才带操作；其余事件只是一行状态。
        let (dot, label, attention) = match self.0 {
            TaskMessage::WorkerNeedsAttention {
                node_id,
                dispatch_id,
            } => {
                let open = ctx.task.workers.iter().any(|worker| {
                    worker.card_id == Some(*node_id)
                        && worker.dispatch_id == *dispatch_id
                        && matches!(
                            worker.phase,
                            WorkerPhase::Failed(WorkerFailure::Unconfirmed)
                        )
                });
                let dot = if open {
                    theme.accent_warm
                } else {
                    theme.text_tertiary
                };
                (
                    dot,
                    t("staff.worker_needs_attention_short"),
                    open.then(|| (*node_id, dispatch_id.clone())),
                )
            }
            TaskMessage::Reopened => (theme.text_tertiary, t("staff.reopened"), None),
            _ => (theme.accent_violet, t("staff.worker_started"), None),
        };
        let muted = attention.is_none();
        let mut row = div()
            .debug_selector(move || format!("staff-event-{index}"))
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(tokens::SPACE_8))
            .text_size(px(tokens::META_TEXT))
            .text_color(rgba(if muted {
                theme.text_tertiary
            } else {
                theme.text_secondary
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(tokens::SPACE_12))
                    .py(px(tokens::SPACE_4))
                    .rounded_full()
                    .bg(rgba(ui::icons::with_alpha(theme.panel_high, 140)))
                    .child(div().size(px(6.)).rounded_full().bg(rgba(dot)))
                    .child(label),
            );
        if let Some((node, dispatch)) = attention {
            row = row
                .child(
                    shell::action_button(ctx, ("staff-redeliver", index), true, cx)
                        .debug_selector(|| "staff-redeliver".into())
                        .label(t("staff.redeliver"))
                        .disabled(ctx.app.staff.busy)
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
                );
        }
        row.into_any_element()
    }
}
