use super::super::*;
use super::shell;
use gpui::rgba;
use ui::component::{Disableable, IconName, StyledExt};
use ui::i18n::text as t;

pub(in crate::app::staff::messages) struct Proposal<'a>(pub &'a TaskMessage);

impl MessageBlock for Proposal<'_> {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let theme = ctx.app.staff_theme();
        let message = self.0;
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        if let TaskMessage::Proposal { proposal_id } = message {
            if let Some(proposal) = task.proposals.iter().find(|p| p.id == *proposal_id) {
                if proposal.ignored {
                    return super::ignored_proposal::render(ctx, proposal, cx);
                }
                let id = task.id.clone();
                let ignore_id = id.clone();
                let proposal_id = *proposal_id;
                let draft = proposal.prompt.clone();
                let dispatched = proposal.dispatch_id.is_some();
                // Material filled card: tonal surface, no outline.
                row = shell::card(ctx, true)
                    .child(
                        shell::header()
                            .text_size(px(tokens::LABEL_TEXT))
                            .font_semibold()
                            .child(crate::app::chrome::brand_logo())
                            .child(t("staff.suggestion"))
                            .child(div().flex_1())
                            .when(!dispatched, |row| {
                                row.child(
                                    div()
                                        .text_size(px(tokens::META_TEXT))
                                        .font_normal()
                                        .text_color(rgba(theme.accent_warm))
                                        .child(t("staff.your_decision")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::SPACE_16))
                            .font_semibold()
                            .child(proposal.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::LABEL_TEXT))
                            .text_color(rgba(theme.text_secondary))
                            .child(proposal.why.clone()),
                    )
                    .child(
                        div()
                            .px(px(tokens::SPACE_16))
                            .py(px(tokens::SPACE_12))
                            .rounded(px(tokens::SPACE_12))
                            .bg(rgba(theme.panel))
                            .text_size(px(tokens::LABEL_TEXT))
                            .child(proposal.prompt.clone()),
                    )
                    .child(
                        div().flex().child(
                            div()
                                .px(px(tokens::SPACE_12))
                                .py(px(tokens::SPACE_4))
                                .rounded_full()
                                .bg(rgba(ui::icons::with_alpha(theme.accent_cyan, 30)))
                                .text_size(px(tokens::SPACE_12))
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
                            .gap(px(tokens::SPACE_8))
                            .child(
                                shell::action_button(ctx, ("staff-adopt", proposal_id), true, cx)
                                    .debug_selector(move || format!("staff-adopt-{proposal_id}"))
                                    .icon(IconName::Check)
                                    .label(if dispatched {
                                        t("staff.dispatched")
                                    } else {
                                        t("staff.adopt")
                                    })
                                    .disabled(ctx.app.staff.busy || dispatched)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.approve_staff_proposal(id.clone(), proposal_id, cx)
                                    })),
                            )
                            .when(!dispatched, |actions| {
                                actions.child(
                                    shell::action_button(
                                        ctx,
                                        ("staff-ignore", proposal_id),
                                        false,
                                        cx,
                                    )
                                    .debug_selector(move || format!("staff-ignore-{proposal_id}"))
                                    .label(t("staff.ignore"))
                                    .disabled(ctx.app.staff.busy || proposal.approved)
                                    .on_click(cx.listener(
                                        move |app, _, _, cx| {
                                            app.set_staff_proposal_ignored(
                                                ignore_id.clone(),
                                                proposal_id,
                                                true,
                                                cx,
                                            )
                                        },
                                    )),
                                )
                            })
                            .when(!dispatched, |actions| {
                                actions.child(
                                    shell::action_button(
                                        ctx,
                                        ("staff-edit-proposal", proposal_id),
                                        false,
                                        cx,
                                    )
                                    .text_size(px(tokens::LABEL_TEXT))
                                    .label(t("staff.edit_proposal"))
                                    .disabled(ctx.app.staff.busy)
                                    .on_click(cx.listener(
                                        move |app, _, window, cx| {
                                            app.edit_staff_proposal(&draft, window, cx);
                                        },
                                    )),
                                )
                            }),
                    );
            }
        }
        row.into_any_element()
    }
}
