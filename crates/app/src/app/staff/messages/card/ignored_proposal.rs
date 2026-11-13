use super::super::*;
use super::shell;
use gpui::rgba;
use norma::protocol::StaffProposal;
use ui::component::{Disableable, IconName};
use ui::i18n::text as t;

pub(super) fn render(
    ctx: &BlockCtx,
    proposal: &StaffProposal,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = proposal.id;
    let key = format!("proposal-{}-{id}", ctx.task.id);
    let expanded = ctx.app.staff.expanded.contains(&key);
    let task_id = ctx.task.id.clone();
    let mut body = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_12))
        .border_1()
        .border_dashed()
        .border_color(rgba(ctx.app.staff_line()))
        .rounded(px(tokens::SPACE_12))
        .p(px(tokens::SPACE_8))
        .child(
            shell::action_button(ctx, ("staff-ignored", id), false, cx)
                .debug_selector(move || format!("staff-ignored-{id}"))
                .w_full()
                .justify_start()
                .icon(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .child(
                    div()
                        .text_color(rgba(ctx.app.staff_theme().text_secondary))
                        .child(t("staff.ignored")),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(proposal.title.clone()),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    if !app.staff.expanded.remove(&key) {
                        app.staff.expanded.insert(key.clone());
                    }
                    app.notify_staff(cx);
                })),
        );
    if expanded {
        body = body.child(div().p(px(tokens::SPACE_8)).child(proposal.prompt.clone()));
        if !ctx.task.proposals.iter().any(|other| other.id > id) {
            body = body.child(
                shell::action_button(ctx, ("staff-restore", id), false, cx)
                    .debug_selector(move || format!("staff-restore-{id}"))
                    .label(t("staff.restore"))
                    .disabled(ctx.app.staff.busy)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.set_staff_proposal_ignored(task_id.clone(), id, false, cx)
                    })),
            );
        }
    }
    body.into_any_element()
}
