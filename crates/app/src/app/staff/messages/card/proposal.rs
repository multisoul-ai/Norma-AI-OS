use super::super::*;
use super::shell;
use gpui::{linear_color_stop, linear_gradient, rgba, Div};
use ui::component::{
    button::ButtonVariants,
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, IconName, StyledExt,
};
use ui::i18n::text as t;

/// 收起时约四行正文加上下内边距。
const PROMPT_COLLAPSED_HEIGHT: f32 = 132.;

/// 超过四行或约四行字数的指令默认收起。
fn prompt_is_long(prompt: &str) -> bool {
    prompt.lines().count() > 4 || prompt.chars().count() > 160
}

pub(in crate::app::staff::messages) struct Proposal(pub u64);

impl MessageBlock for Proposal {
    fn render(
        &self,
        ctx: &BlockCtx,
        _window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) -> AnyElement {
        let task = ctx.task;
        let theme = ctx.app.staff_theme();
        let mut row = div().w_full().flex().flex_col().gap(px(tokens::SPACE_8));
        {
            let proposal_id = &self.0;
            if let Some(proposal) = task.proposals.iter().find(|p| p.id == *proposal_id) {
                if proposal.superseded {
                    return super::superseded_proposal::render(ctx, proposal);
                }
                if proposal.ignored {
                    return super::ignored_proposal::render(ctx, proposal, cx);
                }
                if proposal.dispatch_id.is_some() {
                    return super::dispatched_proposal::render(ctx, proposal, cx);
                }
                let id = task.id.clone();
                let ignore_id = id.clone();
                let proposal_id = *proposal_id;
                let draft = proposal.prompt.clone();
                let dispatched = proposal.dispatch_id.is_some();
                let prompt_key = format!("proposal-prompt-{}-{proposal_id}", task.id);
                let long = prompt_is_long(&proposal.prompt);
                let collapsed = long && !ctx.app.staff.expanded.contains(&prompt_key);
                let panel_color = theme.panel;
                let mut prompt = div()
                    .debug_selector(move || format!("staff-proposal-prompt-{proposal_id}"))
                    .relative()
                    .px(px(tokens::SPACE_16))
                    .py(px(tokens::SPACE_12))
                    .rounded(px(tokens::SPACE_12))
                    .bg(rgba(panel_color))
                    .text_size(px(tokens::LABEL_TEXT))
                    .child(proposal.prompt.clone());
                if collapsed {
                    // 收起时只露约四行，底部渐隐提示还有内容。
                    prompt = prompt
                        .max_h(px(PROMPT_COLLAPSED_HEIGHT))
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(56.))
                                .bg(linear_gradient(
                                    180.,
                                    linear_color_stop(rgba(panel_color & 0xffff_ff00), 0.),
                                    linear_color_stop(rgba(panel_color), 1.),
                                )),
                        );
                }
                // Material filled card: tonal surface, no outline.
                row = shell::card(ctx, true)
                    .debug_selector(move || format!("staff-proposal-card-{proposal_id}"))
                    .child(
                        shell::header()
                            .text_size(px(tokens::META_TEXT))
                            .child(
                                div()
                                    .text_color(rgba(theme.text_secondary))
                                    .child(t("staff.suggestion")),
                            )
                            .child(div().flex_1())
                            .when(!dispatched, |row| {
                                row.child(
                                    div()
                                        .px(px(tokens::SPACE_8))
                                        .py(px(2.))
                                        .rounded_full()
                                        .bg(rgba(ui::icons::with_alpha(theme.accent_warm, 36)))
                                        .font_semibold()
                                        .text_color(rgba(theme.accent_warm))
                                        .child(t("staff.your_decision")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(tokens::SPACE_4))
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
                            .children(proposal.downgrade.as_deref().map(|code| {
                                div()
                                    .text_size(px(tokens::META_TEXT))
                                    .text_color(rgba(theme.accent_warm))
                                    .child(downgrade_text(code))
                            })),
                    )
                    .child(prompt)
                    .when(long, |card| {
                        card.child(
                            div().flex().child(
                                shell::action_button(
                                    ctx,
                                    ("staff-proposal-expand", proposal_id),
                                    false,
                                    cx,
                                )
                                .debug_selector(move || {
                                    format!("staff-proposal-expand-{proposal_id}")
                                })
                                .custom(ctx.app.staff_link_style(cx))
                                .px(px(tokens::SPACE_8))
                                .label(if collapsed {
                                    t("staff.expand_prompt")
                                } else {
                                    t("staff.collapse_prompt")
                                })
                                .on_click(cx.listener(
                                    move |app, _, _, cx| {
                                        if !app.staff.expanded.remove(&prompt_key) {
                                            app.staff.expanded.insert(prompt_key.clone());
                                        }
                                        app.notify_staff(cx);
                                    },
                                )),
                            ),
                        )
                    })
                    .child(
                        // 智能体、模型与三个操作同在卡片底栏；主按钮固定在最右。
                        choice_row(ctx, &task.id, proposal, dispatched || proposal.approved, cx)
                            .flex_wrap()
                            .pt(px(tokens::SPACE_12))
                            .border_t_1()
                            .border_color(rgba(ctx.app.staff_line()))
                            .child(div().flex_1())
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
                                    .debug_selector(move || {
                                        format!("staff-edit-proposal-{proposal_id}")
                                    })
                                    .custom(ctx.app.staff_neutral_style(cx))
                                    .label(t("staff.edit_proposal"))
                                    .disabled(ctx.app.staff.busy)
                                    .on_click(cx.listener(
                                        move |app, _, window, cx| {
                                            app.edit_staff_proposal(&draft, window, cx);
                                        },
                                    )),
                                )
                            })
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
                            ),
                    );
                // 高度只由内容决定，不被消息行的交叉轴拉伸改写，按钮不会落到卡片外。
                row.style().align_self = Some(gpui::AlignSelf::FlexStart);
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
    let engine_label = ctx.app.staff_engine_label(&agent);
    let app = cx.weak_entity();
    let agent_menu = {
        let (task_id, agent, app) = (task_id.to_owned(), agent.clone(), app.clone());
        shell::action_button(ctx, ("staff-agent-pick", proposal_id), false, cx)
            .debug_selector(move || format!("staff-agent-pick-{proposal_id}"))
            .label(ui::i18n::format(
                "staff.proposal_for",
                &[("agent", engine_label.clone())],
            ))
            .dropdown_caret(true)
            .disabled(busy || locked)
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.max_h(px(360.0)).scrollable(true);
                for (name, profile) in &agents {
                    let (app, task_id, pick) = (app.clone(), task_id.clone(), name.clone());
                    menu = menu.item(
                        PopupMenuItem::new(profile_title(profile))
                            .checked(name.eq_ignore_ascii_case(&agent))
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
    norma::orchestration::OrchestrationProvider::ALL
        .into_iter()
        .find(|provider| provider.profile() == *profile)
        .map(|provider| provider.display_name().to_owned())
        .unwrap_or_else(|| format!("{profile:?}"))
}

/// 降级原因码转成面向用户的文案；未知码按风险处理。
fn downgrade_text(code: &str) -> String {
    match code {
        "autonomy_ask" => t("staff.downgrade.autonomy_ask").to_string(),
        "limit_reached" => ui::i18n::format(
            "staff.downgrade.limit_reached",
            &[(
                "limit",
                brain::staff::policy::AUTO_DISPATCH_LIMIT.to_string(),
            )],
        ),
        _ => t("staff.downgrade.risk").to_string(),
    }
}
