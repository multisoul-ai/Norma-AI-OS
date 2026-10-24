//! 运行概览里和后台大脑有关的部分：审批行的“允许 / 拒绝”，以及“Norma 自主处理”栏。

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::{
    OverviewFeedback, OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use super::overview_list::group_empty;
use crate::app::{AutoAction, AutoKind, MultisoulApp, ASSIGN_KEY};
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use std::time::SystemTime;

const COPY: u32 = 0xcfe3f4ff;
const META: u32 = 0x8ea8bfff;
const BRAIN_TINT: u32 = 0xa68bff26;
const BRAIN_TEXT: u32 = 0xcbbcffff;

impl MultisoulApp {
    /// 概览审批行上的“允许 / 拒绝”：不经过大脑，直接在拥有该卡片的项目里处理。
    pub(super) fn decide_overview_approval(
        &mut self,
        area_id: &str,
        node_id: u64,
        run_id: &str,
        request_id: &str,
        allow: bool,
        cx: &mut Context<Self>,
    ) {
        let applied = self.with_overview_session(area_id, cx, |app, _| {
            app.decide_approval_by_user(node_id, run_id, request_id, allow)
        });
        if applied != Some(true) {
            self.desktop_workspace.error =
                Some(ui::i18n::text("chrome.brain_approval_closed").into());
        }
        cx.notify();
    }

    pub(super) fn with_overview_session<R>(
        &mut self,
        area_id: &str,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut MultisoulApp, &mut Context<MultisoulApp>) -> R,
    ) -> Option<R> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id.to_owned()))?
            .clone();
        Some(session.update(cx, run))
    }

    /// “以后这类先问我”：把这类操作加入需要用户确认的清单（派任务这一类则关掉自行派发）。
    fn brain_ask_first(&mut self, key: String, cx: &mut Context<Self>) {
        self.update_brain_settings_with(
            move |settings| {
                if key == ASSIGN_KEY {
                    settings.autonomy.assign_followups = false;
                } else if !settings.always_ask.contains(&key) {
                    settings.always_ask.push(key);
                }
            },
            false,
            cx,
        );
    }

    fn brain_stop_agent(&mut self, area_id: &str, node_id: u64, cx: &mut Context<Self>) {
        let stopped = self
            .with_overview_session(area_id, cx, |app, _| app.interrupt_agent_by_user(node_id))
            .unwrap_or(false);
        if !stopped {
            self.desktop_workspace.error =
                Some(ui::i18n::text("chrome.brain_agent_not_found").into());
        }
        cx.notify();
    }

    fn toggle_brain_why(&mut self, area_id: &str, id: u64, cx: &mut Context<Self>) {
        let key = (area_id.to_owned(), id);
        let next = if self.brain_why() == Some(&key) {
            None
        } else {
            Some(key)
        };
        self.set_brain_why(next);
        cx.notify();
    }

    /// “Norma 自主处理”栏的内容：最近自行放行或派发的事项，每条带三个操作。
    pub(super) fn render_brain_group_body(
        &mut self,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let actions = self.brain_overview().actions.clone();
        let open = self.brain_why().cloned();
        if actions.is_empty() {
            return div()
                .id("desktop-run-body-brain")
                .flex_1()
                .min_h(px(0.0))
                .child(group_empty(OverviewGroup::Brain, true));
        }
        let mut body = div()
            .id("desktop-run-body-brain")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0));
        for (area_id, action) in actions {
            let expanded = open.as_ref() == Some(&(area_id.clone(), action.id));
            body = body.child(self.render_brain_action(&area_id, &action, expanded, feedback, cx));
        }
        body
    }

    fn render_brain_action(
        &mut self,
        area_id: &str,
        action: &AutoAction,
        expanded: bool,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Div {
        let verb = match action.kind {
            AutoKind::Approved => "放行",
            AutoKind::Denied => "拒绝",
            AutoKind::Assigned | AutoKind::UserDispatched => "派发",
        };
        // 用户自己批准的派发没有“以后先问我”的意义。
        let by_user = action.kind == AutoKind::UserDispatched;
        let id = action.id;
        let node_id = action.node_id;
        let key = action.ask_key.clone();
        let area = area_id.to_owned();
        let why = pill(
            ElementId::Name(format!("brain-action-why-{id}").into()),
            if expanded { "收起" } else { "为什么" },
            PillStyle::Quiet,
        )
        .on_click({
            let area = area.clone();
            cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_brain_why(&area, id, cx);
            })
        });
        let ask = pill(
            ElementId::Name(format!("brain-action-ask-{id}").into()),
            "以后这类先问我",
            PillStyle::Quiet,
        )
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            app.brain_ask_first(key.clone(), cx);
        }));
        let stop = pill(
            ElementId::Name(format!("brain-action-stop-{id}").into()),
            "停止该 Agent",
            if node_id == 0 {
                PillStyle::Disabled
            } else {
                PillStyle::Quiet
            },
        )
        .when(node_id != 0, |pill| {
            pill.on_click({
                let area = area.clone();
                cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.brain_stop_agent(&area, node_id, cx);
                })
            })
        });
        let meta = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(OVERVIEW_TEXT_COPY))
                    .text_color(rgba(META))
                    .child(format!("{} · {}", action.agent, ago(action.at))),
            )
            .child(why);
        div()
            .w_full()
            .flex_none()
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .rounded(px(9.0))
            .hover(move |style| style.bg(rgba(feedback.hover)))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(5.0))
                            .bg(rgba(BRAIN_TINT))
                            .text_size(px(OVERVIEW_TEXT_SECONDARY))
                            .text_color(rgba(BRAIN_TEXT))
                            .child(ui::i18n::text(if by_user {
                                "chrome.brain_user_badge"
                            } else {
                                "chrome.brain_auto_badge"
                            })),
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
                                    .child(format!("{verb}：{}", action.summary)),
                            )
                            .child(meta),
                    ),
            )
            .when(expanded, |row| {
                row.child(
                    div()
                        .pl(px(8.0))
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(COPY))
                        .child(ui::i18n::format(
                            "chrome.brain_reason",
                            &[("reason", action.reason.clone())],
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .when(!by_user, |row| row.child(ask))
                        .child(stop),
                )
            })
    }
}

/// “3 分钟前”这样的相对时间。
fn ago(at: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(at)
        .map_or(0, |d| d.as_secs());
    match seconds {
        0..=59 => "刚刚".to_string(),
        60..=3599 => format!("{} 分钟前", seconds / 60),
        _ => format!("{} 小时前", seconds / 3600),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PillStyle {
    Primary,
    Quiet,
    Disabled,
}

/// 概览里的小胶囊按钮：整块可点，悬停与按下各有一档颜色；与想法清单的胶囊同一做法。
pub(super) fn pill(
    id: impl Into<ElementId>,
    label: &'static str,
    style: PillStyle,
) -> Stateful<Div> {
    let (fill, hover, pressed, text) = match style {
        PillStyle::Primary => (0x58adff40, 0x58adff66, 0x58adff88, 0xeef6ffff),
        PillStyle::Quiet => (0xcde8ff14, 0xcde8ff2e, 0xcde8ff44, 0xcfe3f4ff),
        PillStyle::Disabled => (0xcde8ff0a, 0xcde8ff0a, 0xcde8ff0a, 0x8ea8bf99),
    };
    div()
        .id(id)
        .h(px(22.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .rounded(px(6.0))
        .bg(rgba(fill))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(text))
        .when(style != PillStyle::Disabled, |pill| {
            pill.cursor_pointer()
                .hover(move |s| s.bg(rgba(hover)))
                .active(move |s| s.bg(rgba(pressed)))
        })
        .child(label)
}
