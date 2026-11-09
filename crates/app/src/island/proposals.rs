//! 幕僚提议卡。不进主动弹出，只出现在「需要你处理」、头像和队列里。

use super::{
    actions::IslandTarget,
    controller::{proposal_key, IslandController},
    style::{self, Tone},
    view::IslandView,
    IslandProject, IslandProposalBlock, IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use ui::{component::Disableable, i18n::text};

/// 派发失败的结构化原因。句子在岛上翻译。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProposalDispatchError {
    Missing,
    Blocked {
        block: IslandProposalBlock,
        agent: String,
    },
    Failed(String),
}

impl IslandController {
    pub(super) fn dispatch_proposal(
        &mut self,
        route: IslandTarget,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        let key = proposal_key(route.source, &route.workspace, &route.area, id);
        if self.pending.contains(&key) {
            return;
        }
        let Some(session) = route.session(cx) else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        self.pending.insert(key.clone());
        let result = session.update(cx, |app, _| app.dispatch_brain_proposal(id));
        self.finish_proposal(route, key, result, true, cx);
    }

    pub(super) fn dismiss_proposal(
        &mut self,
        route: IslandTarget,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        let key = proposal_key(route.source, &route.workspace, &route.area, id);
        if self.pending.contains(&key) {
            return;
        }
        let Some(session) = route.session(cx) else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        self.pending.insert(key.clone());
        let result = session.update(cx, |app, _| {
            if app.decline_brain_proposal(id) {
                Ok(())
            } else {
                Err(ProposalDispatchError::Missing)
            }
        });
        self.finish_proposal(route, key, result, false, cx);
    }

    fn finish_proposal(
        &mut self,
        route: IslandTarget,
        key: String,
        result: Result<(), ProposalDispatchError>,
        dispatched: bool,
        cx: &mut Context<Self>,
    ) {
        self.pending.remove(&key);
        self.message = Some(match result {
            Ok(()) => {
                self.resolved_proposals.insert(key);
                if let Some(host) = route.host(cx) {
                    host.update(cx, |app, cx| app.note_workspace_activity_changed(cx));
                }
                ui::i18n::LocalizedText::new(if dispatched {
                    "island.proposal_dispatched"
                } else {
                    "island.proposal_dismissed"
                })
            }
            Err(ProposalDispatchError::Blocked { block, agent }) => block_text(block, &agent),
            Err(ProposalDispatchError::Failed(reason)) => {
                ui::i18n::LocalizedText::new("island.proposal_failed").with_arg("reason", reason)
            }
            Err(ProposalDispatchError::Missing) => ui::i18n::LocalizedText::new("island.expired"),
        });
        cx.notify();
    }
}

impl IslandView {
    pub(super) fn render_proposals(
        &mut self,
        workspace: &IslandWorkspace,
        project: &IslandProject,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let route = IslandTarget::new(workspace, project);
        let mut rows = Vec::new();
        for proposal in &project.proposals {
            let key = proposal_key(route.source, &route.workspace, &route.area, proposal.id);
            let waiting = self.controller.read(cx).pending.contains(&key);
            let blocked = proposal.block;
            let mut actions = div().flex().items_center().justify_end().gap(px(6.));
            let dispatch_route = route.clone();
            let dismiss_route = route.clone();
            let id = proposal.id;
            actions = actions.child(
                style::action(
                    format!("proposal-dispatch-{key}"),
                    text("island.proposal_dispatch"),
                    Tone::Primary,
                    cx,
                )
                .disabled(waiting || blocked.is_some())
                .on_click(cx.listener(move |view, _, _, cx| {
                    cx.stop_propagation();
                    view.controller.update(cx, |controller, cx| {
                        controller.dispatch_proposal(dispatch_route.clone(), id, cx);
                    });
                })),
            );
            actions = actions.child(
                style::action(
                    format!("proposal-dismiss-{key}"),
                    text("island.proposal_ignore"),
                    Tone::Neutral,
                    cx,
                )
                .disabled(waiting)
                .on_click(cx.listener(move |view, _, _, cx| {
                    cx.stop_propagation();
                    view.controller.update(cx, |controller, cx| {
                        controller.dismiss_proposal(dismiss_route.clone(), id, cx);
                    });
                })),
            );
            let mut card = style::card().child(style::card_heading(
                &proposal.agent,
                &proposal.agent,
                format!("{} · {}", workspace.name, project.name),
                &proposal.title,
                text("island.proposal_label"),
                style::ATTENTION,
                runtime::island::bloub::Mood::Waiting,
            ));
            if !proposal.why.is_empty() {
                card = card.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(style::MUTED))
                        .child(brief_why(&proposal.why)),
                );
            }
            if let Some(block) = blocked {
                card = card.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(style::ATTENTION))
                        .child(block_line(block, &proposal.agent)),
                );
            }
            rows.push(card.child(actions).into_any_element());
        }
        rows
    }
}

fn block_text(block: IslandProposalBlock, agent: &str) -> ui::i18n::LocalizedText {
    ui::i18n::LocalizedText::new(block_key(block)).with_arg("agent", agent.to_owned())
}

fn block_line(block: IslandProposalBlock, agent: &str) -> String {
    ui::i18n::format(block_key(block), &[("agent", agent.to_owned())])
}

fn block_key(block: IslandProposalBlock) -> &'static str {
    match block {
        IslandProposalBlock::WaitingConfirmation => "island.proposal_waiting",
        IslandProposalBlock::Busy => "island.proposal_busy",
        IslandProposalBlock::Missing => "island.proposal_missing",
        IslandProposalBlock::Disabled => "island.proposal_disabled",
    }
}

/// 理由只留一行。42 个字符之后加省略号，短句原样保留。
#[cfg(test)]
#[path = "../../tests/unit/island_proposal_copy_tests.rs"]
mod proposal_copy_tests;

fn brief_why(why: &str) -> String {
    let mut chars = why.chars();
    let head: String = chars.by_ref().take(42).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}
