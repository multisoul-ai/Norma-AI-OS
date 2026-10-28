//! “需要你处理”里大脑提议的下一步：标题行右侧始终是“拒绝 / 采纳”，点行展开完整指令与理由。
//! 派发和忽略都在拥有该提议的项目会话里执行，不经过大脑。

use super::super::overview_rows::{
    OverviewFeedback, OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use super::overview_brain::{pill, PillStyle};
use crate::app::{MultisoulApp, OverviewProposal, ProposalFeedback};
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};

/// 收起时的行高：标题一行 + 说明一行。
const PROPOSAL_ROW_HEIGHT: f32 = 52.0;
/// 展开后多出指令（最多约三行）与理由一行，栏高按它分配，免得内容被栏裁掉。
const PROPOSAL_EXPANDED_HEIGHT: f32 = 132.0;
const PROPOSAL_FEEDBACK_HEIGHT: f32 = 48.0;
const COPY: u32 = 0xcfe3f4ff;
const META: u32 = 0x8ea8bfff;
const TINT: u32 = 0xf2c77326;
const TEXT: u32 = 0xf6dfaaff;

impl MultisoulApp {
    /// 各条提议在栏里占的高度，用来算“需要你处理”栏的起始高度。
    pub(super) fn proposal_heights(&self) -> Vec<f32> {
        let open = self.brain_proposal_open();
        self.brain_overview()
            .proposals
            .iter()
            .map(|item| {
                let height = if open == Some(&(item.area_id.clone(), item.proposal.id)) {
                    PROPOSAL_EXPANDED_HEIGHT
                } else {
                    PROPOSAL_ROW_HEIGHT
                };
                if self.proposal_hint(item).is_some() {
                    height + PROPOSAL_FEEDBACK_HEIGHT
                } else {
                    height
                }
            })
            .collect()
    }

    fn proposal_hint(&self, item: &OverviewProposal) -> Option<String> {
        match self.brain_proposal_feedback(&item.area_id, item.proposal.id) {
            Some(ProposalFeedback::Failed(message)) => Some(message.clone()),
            Some(ProposalFeedback::Dispatching) => None,
            None => item
                .block
                .as_ref()
                .map(|block| block.message(&item.proposal.agent)),
        }
    }

    pub(super) fn render_proposal_rows(
        &mut self,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let items = self.brain_overview().proposals.clone();
        let open = self.brain_proposal_open().cloned();
        items
            .iter()
            .map(|item| {
                let key = (item.area_id.clone(), item.proposal.id);
                let expanded = open.as_ref() == Some(&key);
                self.render_proposal(item, expanded, feedback, cx)
            })
            .collect()
    }

    fn render_proposal(
        &mut self,
        item: &OverviewProposal,
        expanded: bool,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = item.proposal.id;
        let area = item.area_id.clone();
        let tag = format!("{area}-{id}");
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let target = ui::i18n::format(
            if item.new_card {
                "chrome.brain_proposal_new_card"
            } else {
                "chrome.brain_proposal_to"
            },
            &[("agent", item.proposal.agent.clone())],
        );
        // 采纳 / 拒绝始终在标题行右侧；点行其余部分展开详情。
        let hint = self.proposal_hint(item);
        let aside = self.proposal_buttons(item, &tag, cx);
        let selector = format!("desktop-brain-proposal-{id}");
        let head = div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .flex_none()
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(5.0))
                    .bg(rgba(TINT))
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .text_color(rgba(TEXT))
                    .child(ui::i18n::text("chrome.brain_proposal_badge")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(OVERVIEW_TEXT_PRIMARY))
                            .text_color(rgba(COPY))
                            .truncate()
                            .child(item.proposal.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(OVERVIEW_TEXT_COPY))
                            .text_color(rgba(META))
                            .child(target),
                    ),
            )
            .child(aside);
        div()
            .id(ElementId::Name(format!("brain-proposal-{tag}").into()))
            .debug_selector(move || selector.clone())
            .w_full()
            .flex_none()
            .min_h(px(PROPOSAL_ROW_HEIGHT))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .rounded(px(9.0))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .on_click({
                let area = area.clone();
                cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.toggle_brain_proposal((area.clone(), id));
                    cx.notify();
                })
            })
            .child(head)
            .when_some(hint, |row, message| {
                row.child(
                    div()
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(TEXT))
                        .line_clamp(3)
                        .child(message),
                )
            })
            .when(expanded, |row| {
                row.child(
                    div()
                        .pl(px(8.0))
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(COPY))
                        .child(item.proposal.task.clone()),
                )
                .child(
                    div()
                        .pl(px(8.0))
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(META))
                        .child(ui::i18n::format(
                            "chrome.brain_reason",
                            &[("reason", item.proposal.why.clone())],
                        )),
                )
            })
    }

    fn proposal_buttons(
        &mut self,
        item: &OverviewProposal,
        tag: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let area = &item.area_id;
        let id = item.proposal.id;
        let pending = matches!(
            self.brain_proposal_feedback(area, id),
            Some(ProposalFeedback::Dispatching)
        );
        let enabled = !pending && item.block.is_none();
        let ignore = pill(
            ElementId::Name(format!("brain-proposal-ignore-{tag}").into()),
            ui::i18n::text("chrome.brain_proposal_ignore"),
            if pending {
                PillStyle::Disabled
            } else {
                PillStyle::Quiet
            },
        )
        .when(!pending, |button| {
            button.on_click({
                let area = area.to_owned();
                cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.decline_overview_proposal(&area, id, cx);
                })
            })
        });
        let dispatch = pill(
            ElementId::Name(format!("brain-proposal-dispatch-{tag}").into()),
            if pending {
                ui::i18n::text("chrome.brain_proposal_dispatching")
            } else if let Some(block) = &item.block {
                block.label()
            } else {
                ui::i18n::text("chrome.brain_proposal_dispatch")
            },
            if enabled {
                PillStyle::Primary
            } else {
                PillStyle::Disabled
            },
        )
        .when(enabled, |button| {
            button.on_click({
                let area = area.to_owned();
                cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_overview_proposal(&area, id, cx).detach();
                })
            })
        })
        // Disabled pills still consume the click, so it cannot expand the row underneath.
        .when(!enabled, |button| {
            button
                .cursor_default()
                .on_click(|_, _, cx| cx.stop_propagation())
        });
        div()
            .flex_none()
            .flex()
            .gap(px(4.0))
            .child(ignore)
            .child(dispatch)
    }

    pub(crate) fn dispatch_overview_proposal(
        &mut self,
        area_id: &str,
        id: u64,
        cx: &mut Context<Self>,
    ) -> gpui::Task<()> {
        if matches!(
            self.brain_proposal_feedback(area_id, id),
            Some(ProposalFeedback::Dispatching)
        ) {
            return gpui::Task::ready(());
        }
        self.set_brain_proposal_feedback(area_id, id, Some(ProposalFeedback::Dispatching));
        cx.notify();
        let workspace = self.desktop_workspace.active_id.clone();
        let prepared = self.with_overview_session(area_id, cx, |app, cx| {
            app.shared_context_service()
                .map(|service| (cx.entity().entity_id(), service))
        });
        let (source, service) = match prepared {
            Some(Ok(prepared)) => prepared,
            other => {
                let message = match other {
                    Some(Err(error)) => error.to_string(),
                    _ => ui::i18n::text("chrome.brain_proposal_closed").to_owned(),
                };
                self.set_brain_proposal_feedback(
                    area_id,
                    id,
                    Some(ProposalFeedback::Failed(message.clone())),
                );
                self.desktop_workspace.error = Some(message.into());
                cx.notify();
                return gpui::Task::ready(());
            }
        };
        let area_id = area_id.to_owned();
        cx.spawn(async move |this, cx| {
            let timeout = cx
                .background_executor()
                .timer(std::time::Duration::from_secs(2));
            let loaded = match futures::future::select(
                Box::pin(service.snapshot()),
                Box::pin(timeout),
            )
            .await
            {
                futures::future::Either::Left((result, _)) => {
                    result.map(|_| ()).map_err(|error| error.to_string())
                }
                futures::future::Either::Right(_) => {
                    Err("context_prepare_timeout：状态读取超时，请稍后重试".into())
                }
            };
            let dispatched = this.update(cx, |host, cx| {
                // A late load must not dispatch into another workspace or replacement session.
                if host.desktop_workspace.active_id != workspace {
                    host.set_brain_proposal_feedback(&area_id, id, None);
                    return None;
                }
                if host.with_overview_session(&area_id, cx, |_, cx| cx.entity().entity_id())
                    != Some(source)
                {
                    host.set_brain_proposal_feedback(&area_id, id, None);
                    host.desktop_workspace.error =
                        Some("context_scope_mismatch：来源项目已改变".into());
                    cx.notify();
                    return None;
                }
                let result = loaded.and_then(|()| {
                    host.with_overview_session(&area_id, cx, |app, cx| {
                        if cx.entity().entity_id() != source
                            || !app
                                .shared_context_service()
                                .is_ok_and(|current| current.scope() == service.scope())
                        {
                            return Err("context_scope_mismatch：来源项目已改变".into());
                        }
                        let result = app.dispatch_brain_proposal(id);
                        cx.notify();
                        result
                    })
                    .unwrap_or_else(|| {
                        Err(ui::i18n::text("chrome.brain_proposal_closed").to_string())
                    })
                });
                if let Err(message) = &result {
                    host.desktop_workspace.error = Some(message.clone().into());
                }
                host.set_brain_proposal_feedback(
                    &area_id,
                    id,
                    result
                        .as_ref()
                        .err()
                        .map(|message| ProposalFeedback::Failed(message.clone())),
                );
                host.refresh_brain_overview(cx);
                cx.notify();
                result.ok()
            });
            if let Ok(Some((op, mut receipt))) = dispatched {
                // The proposal is already consumed. Saving sources cannot replay its action.
                if !service.save_brain_action_result(op, &mut receipt).await {
                    let _ = this.update(cx, |host, cx| {
                        if host.desktop_workspace.active_id != workspace {
                            return;
                        }
                        let current = host
                            .with_overview_session(&area_id, cx, |app, cx| {
                                cx.entity().entity_id() == source
                                    && app
                                        .shared_context_service()
                                        .is_ok_and(|current| current.scope() == service.scope())
                            })
                            .unwrap_or(false);
                        if current {
                            host.desktop_workspace.error =
                                Some("任务已接收，但来源未确认保存；请勿重复派发".into());
                            cx.notify();
                        }
                    });
                }
            }
        })
    }

    fn decline_overview_proposal(&mut self, area_id: &str, id: u64, cx: &mut Context<Self>) {
        if matches!(
            self.brain_proposal_feedback(area_id, id),
            Some(ProposalFeedback::Dispatching)
        ) {
            return;
        }
        let declined = self
            .with_overview_session(area_id, cx, |app, _| app.decline_brain_proposal(id))
            .unwrap_or(false);
        if !declined {
            self.desktop_workspace.error =
                Some(ui::i18n::text("chrome.brain_proposal_closed").into());
        }
        self.refresh_brain_overview(cx);
        cx.notify();
    }
}
