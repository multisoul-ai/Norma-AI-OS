//! “需要你处理”里大脑提议的下一步：收起时一行标题，展开后显示完整指令、理由与“忽略 / 派发”。
//! 派发和忽略都在拥有该提议的项目会话里执行，不经过大脑。

use super::super::overview_rows::{
    OverviewFeedback, OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use super::overview_brain::{pill, PillStyle};
use crate::app::{MultisoulApp, OverviewProposal};
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};

/// 收起时的行高：标题一行 + 说明一行。
const PROPOSAL_ROW_HEIGHT: f32 = 52.0;
/// 展开后多出指令（最多约三行）与理由一行，栏高按它分配，免得内容被栏裁掉。
const PROPOSAL_EXPANDED_HEIGHT: f32 = 132.0;
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
                if open == Some(&(item.area_id.clone(), item.proposal.id)) {
                    PROPOSAL_EXPANDED_HEIGHT
                } else {
                    PROPOSAL_ROW_HEIGHT
                }
            })
            .collect()
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
        // 展开后按钮放在标题行右侧：栏有高度上限，放在指令下面常常要滚动才看得到。
        let aside = if expanded {
            self.proposal_buttons(&area, id, &tag, cx)
        } else {
            div()
                .flex_none()
                .text_size(px(OVERVIEW_TEXT_COPY))
                .text_color(rgba(META))
                .child(ui::i18n::text("chrome.brain_proposal_expand"))
        };
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

    fn proposal_buttons(&mut self, area: &str, id: u64, tag: &str, cx: &mut Context<Self>) -> Div {
        let ignore = pill(
            ElementId::Name(format!("brain-proposal-ignore-{tag}").into()),
            ui::i18n::text("chrome.brain_proposal_ignore"),
            PillStyle::Quiet,
        )
        .on_click({
            let area = area.to_owned();
            cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.decline_overview_proposal(&area, id, cx);
            })
        });
        let dispatch = pill(
            ElementId::Name(format!("brain-proposal-dispatch-{tag}").into()),
            ui::i18n::text("chrome.brain_proposal_dispatch"),
            PillStyle::Primary,
        )
        .on_click({
            let area = area.to_owned();
            cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_overview_proposal(&area, id, cx);
            })
        });
        div()
            .flex_none()
            .flex()
            .gap(px(4.0))
            .child(ignore)
            .child(dispatch)
    }

    fn dispatch_overview_proposal(&mut self, area_id: &str, id: u64, cx: &mut Context<Self>) {
        let result = self.with_overview_session(area_id, cx, |app, cx| {
            let result = app.dispatch_brain_proposal(id);
            cx.notify();
            result
        });
        match result {
            Some(Ok(())) => {}
            Some(Err(message)) => self.desktop_workspace.error = Some(message.into()),
            None => {
                self.desktop_workspace.error =
                    Some(ui::i18n::text("chrome.brain_proposal_closed").into());
            }
        }
        self.refresh_brain_overview(cx);
        cx.notify();
    }

    fn decline_overview_proposal(&mut self, area_id: &str, id: u64, cx: &mut Context<Self>) {
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
