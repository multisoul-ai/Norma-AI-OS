use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::StaffProposal;
use ui::component::{Disableable, StyledExt};
use ui::i18n::text as t;

/// Dispatched proposal: only the task name and the agent card name remain (prototype `.proposal-sent`).
pub(super) fn render(
    ctx: &BlockCtx,
    proposal: &StaffProposal,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let theme = ctx.app.staff_theme();
    let id = proposal.id;
    let worker = proposal
        .dispatch_id
        .as_deref()
        .and_then(|dispatch| ctx.task.worker_for_dispatch(dispatch));
    let engine = worker.map_or("", |w| w.engine.as_str());
    let mood = worker.map_or(runtime::island::bloub::Mood::Idle, |w| {
        crate::app::staff::detail::worker_mood(w.chip(), w.outcome)
    });
    let running = proposal.auto && worker.is_some_and(|w| w.outcome.is_none());
    let (task_id, proposal_id) = (ctx.task.id.clone(), proposal.id);
    shell::card(ctx, false)
        .debug_selector(move || format!("staff-proposal-card-{id}"))
        .flex_row()
        .items_center()
        .child(crate::island::pet(
            &proposal.agent,
            engine,
            tokens::AVATAR_SIZE,
            mood,
            !ctx.app.motion.reduce_motion,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(tokens::BODY_TEXT))
                .font_semibold()
                .child(proposal.title.clone()),
        )
        .child(
            div()
                .text_size(px(tokens::META_TEXT))
                .text_color(rgba(theme.text_secondary))
                .child(if engine.is_empty() {
                    proposal.agent.clone()
                } else {
                    format!("{} · {engine}", proposal.agent)
                }),
        )
        .child(
            div()
                .text_size(px(tokens::META_TEXT))
                .text_color(rgba(theme.success))
                .child(format!(
                    "✓ {}",
                    t(if proposal.auto {
                        "staff.dispatched_by_staff"
                    } else {
                        "staff.dispatched"
                    })
                )),
        )
        .when(running, |row| {
            row.child(
                shell::action_button(ctx, ("staff-cancel-dispatch", id), false, cx)
                    .debug_selector(move || format!("staff-cancel-dispatch-{id}"))
                    .label(t("staff.cancel_dispatch"))
                    .disabled(ctx.app.staff.busy)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.cancel_staff_worker(&task_id, proposal_id, window, cx)
                    })),
            )
        })
        .into_any_element()
}
