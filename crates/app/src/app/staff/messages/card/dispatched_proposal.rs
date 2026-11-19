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
        crate::app::staff::detail::worker_mood(w.chip(), w.outcome())
    });
    let node = worker.and_then(|w| w.card_id);
    let withdrawable = worker.is_some_and(|w| brain::staff::worker_life::before_running(w.phase));
    let (task_id, proposal_id) = (ctx.task.id.clone(), proposal.id);
    let dispatched = t(if proposal.auto {
        "staff.dispatched_by_staff"
    } else {
        "staff.dispatched"
    });
    let status_text = if worker.is_none() && proposal.dispatch_id.is_some() {
        t("staff.worker_gone").to_owned()
    } else {
        format!("✓ {dispatched}")
    };
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
                .child(status_text),
        )
        .when(withdrawable, |row| {
            row.child(
                shell::action_button(ctx, ("staff-cancel-dispatch", id), false, cx)
                    .debug_selector(move || format!("staff-cancel-dispatch-{id}"))
                    .label(t("staff.cancel_dispatch"))
                    .disabled(ctx.app.staff.busy)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.cancel_staff_worker(&task_id, proposal_id, window, cx)
                    })),
            )
        })
        .id(("staff-open-dispatch", id))
        .on_click(cx.listener(move |app, _, window, cx| {
            if let Some(node) = node {
                app.locate_staff_worker(node, window, cx);
            }
        }))
        .into_any_element()
}
