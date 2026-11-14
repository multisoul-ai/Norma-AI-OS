use super::super::*;
use super::shell;
use gpui::{rgba, Div};
use ui::component::{
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, IconName, StyledExt,
};
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
                    .debug_selector(move || format!("staff-proposal-card-{proposal_id}"))
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
                    .child(choice_row(
                        ctx,
                        &task.id,
                        proposal,
                        dispatched || proposal.approved,
                        cx,
                    ))
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

/// 智能体与模型两个下拉；采纳或派发后锁定，只显示最终值。
fn choice_row(
    ctx: &BlockCtx,
    task_id: &str,
    proposal: &norma::protocol::StaffProposal,
    locked: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let proposal_id = proposal.id;
    let agent = ctx.app.staff_proposal_agent(task_id, proposal);
    let models = ctx.app.staff_model_choices(&agent);
    let key = (task_id.to_owned(), proposal_id);
    let selected = if locked {
        proposal.model.clone()
    } else {
        ctx.app.staff.proposal_models.get(&key).cloned()
    };
    let model_label = selected
        .as_ref()
        .map(|id| {
            models
                .iter()
                .find(|(model, _)| model == id)
                .map_or_else(|| id.clone(), |(_, name)| name.clone())
        })
        .unwrap_or_else(|| t("staff.model_default").to_string());
    let busy = ctx.app.staff.busy;
    let agents = ctx.app.staff_agent_choices();
    let app = cx.weak_entity();
    let agent_menu = {
        let (task_id, agent, app) = (task_id.to_owned(), agent.clone(), app.clone());
        shell::action_button(ctx, ("staff-agent-pick", proposal_id), false, cx)
            .debug_selector(move || format!("staff-agent-pick-{proposal_id}"))
            .label(ui::i18n::format(
                "staff.proposal_for",
                &[("agent", agent.clone())],
            ))
            .dropdown_caret(true)
            .disabled(busy || locked)
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.max_h(px(360.0)).scrollable(true);
                for (name, profile) in &agents {
                    let (app, task_id, pick) = (app.clone(), task_id.clone(), name.clone());
                    menu = menu.item(
                        PopupMenuItem::new(format!("{name} · {}", profile_title(profile)))
                            .checked(*name == agent)
                            .on_click(move |_, _, cx| {
                                let (task_id, pick) = (task_id.clone(), pick.clone());
                                let _ = app.update(cx, |app, cx| {
                                    app.set_staff_proposal_agent(&task_id, proposal_id, pick, cx)
                                });
                            }),
                    );
                }
                menu
            })
    };
    let row = div()
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_8))
        .child(agent_menu);
    if models.is_empty() {
        return row;
    }
    let task_id = task_id.to_owned();
    row.child(
        shell::action_button(ctx, ("staff-model-pick", proposal_id), false, cx)
            .debug_selector(move || format!("staff-model-pick-{proposal_id}"))
            .label(model_label)
            .dropdown_caret(true)
            .disabled(busy || locked)
            .dropdown_menu(move |menu, _, _| {
                let pick = |value: Option<String>| {
                    let (app, task_id) = (app.clone(), task_id.clone());
                    move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                        let (task_id, value) = (task_id.clone(), value.clone());
                        let _ = app.update(cx, |app, cx| {
                            app.set_staff_proposal_model(&task_id, proposal_id, value, cx)
                        });
                    }
                };
                let mut menu = menu.max_h(px(360.0)).scrollable(true).item(
                    PopupMenuItem::new(t("staff.model_default"))
                        .checked(selected.is_none())
                        .on_click(pick(None)),
                );
                for (id, name) in &models {
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(selected.as_deref() == Some(id.as_str()))
                            .on_click(pick(Some(id.clone()))),
                    );
                }
                menu
            }),
    )
}

fn profile_title(profile: &norma::protocol::AgentProfile) -> String {
    format!("{profile:?}")
}
