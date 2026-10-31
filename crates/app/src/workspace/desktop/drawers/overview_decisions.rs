//! Approval controls remain on the outer row; only the request details fold.
use super::super::overview_rows::OverviewRow;
use super::overview_details::DetailKey;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::{
    button::{Button, ButtonVariants},
    Sizable,
};

pub(super) fn detail_key(row: &OverviewRow) -> Option<DetailKey> {
    let (area_id, node_id, run_id, request_id) = decision_target(row)?;
    Some(DetailKey::Approval {
        area_id,
        node_id,
        run_id,
        request_id,
    })
}

impl MultisoulApp {
    pub(super) fn render_approval_overview_row(
        &self,
        row: &OverviewRow,
        index: usize,
        focused: bool,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = detail_key(row).expect("approvable overview row");
        let expanded = self.desktop_workspace.attention.is_open(&key);
        let pending = self.desktop_workspace.attention.pending.contains(&key);
        let decided = self
            .desktop_workspace
            .attention
            .decided
            .iter()
            .find(|(done, _)| done == &key)
            .map(|(_, allow)| *allow);
        let selector = format!("desktop-overview-row-{index}");
        let head = super::overview_detail_view::header(
            format!("approval-toggle-{index}"),
            key,
            super::overview_detail_view::title(&row.primary, "chrome.brain_approval_title"),
            row.secondary.clone(),
            "◇",
            theme.accent_warm,
            self,
            cx,
        );
        let details = self.desktop_workspace.attention.approval_details.clone();
        let action = row.action.clone();
        div()
            .id(ElementId::Name(selector.clone().into()))
            .debug_selector(move || selector)
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .on_key_down(super::overview_detail_view::isolate_navigation)
            .rounded(px(8.))
            .border_1()
            .border_color(rgba(if focused {
                theme.focused_stroke
            } else {
                theme.node_stroke
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pr(px(6.))
                    .child(head)
                    .when(decided.is_none() && !pending, |head| {
                        head.children(decision_buttons(theme, row, index, cx))
                    })
                    .when(pending, |head| {
                        head.child(super::overview_brain::pill(
                            theme,
                            ElementId::Name(format!("overview-pending-{index}").into()),
                            ui::i18n::text("chrome.brain_approval_submitting"),
                            super::overview_brain::PillStyle::Disabled,
                            cx,
                        ))
                    })
                    .when_some(decided, |head, allow| {
                        head.child(
                            div()
                                .pr(px(4.))
                                .text_size(px(11.))
                                .text_color(rgba(theme.success))
                                .child(ui::i18n::text(if allow {
                                    "chrome.brain_approval_allowed"
                                } else {
                                    "chrome.brain_approval_denied"
                                })),
                        )
                    }),
            )
            .when(expanded, |view| {
                let mut body = super::overview_detail_view::body(theme);
                if let Some((what, original)) = details {
                    body = body
                        .child(div().font_family("Menlo").text_size(px(11.)).child(what))
                        .child(
                            div()
                                .text_size(px(11.))
                                .child(ui::i18n::text("chrome.brain_once_scope")),
                        )
                        .child(super::overview_detail_view::original(
                            format!("approval-original-{index}"),
                            "chrome.brain_request_details",
                            original,
                            self,
                            cx,
                        ));
                } else {
                    body = body.child(ui::i18n::text("chrome.brain_request_at_source"));
                }
                view.child(
                    body.child(
                        Button::new(ElementId::Name(format!("approval-source-{index}").into()))
                            .small()
                            .ghost()
                            .label(ui::i18n::text("chrome.question_native"))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.activate_overview_action(action.clone(), window, cx);
                            })),
                    ),
                )
            })
    }
}

/// 概览里可直接处理的审批：（项目区域，卡片，run，请求标识）。没有请求标识或行不可用时没有。
pub(super) fn decision_target(row: &OverviewRow) -> Option<(String, u64, String, String)> {
    use super::super::overview_rows::OverviewAction;
    let OverviewAction::LocateApproval {
        area_id,
        node_id,
        run_id,
        request_id: Some(request_id),
        ..
    } = &row.action
    else {
        return None;
    };
    row.enabled.then(|| {
        (
            area_id.clone(),
            *node_id,
            run_id.clone(),
            request_id.clone(),
        )
    })
}

/// 能直接处理的审批行右侧的“允许 / 拒绝”。点击只处理审批，不触发整行的“定位”。
pub(super) fn decision_buttons(
    theme: ui::Theme,
    row: &OverviewRow,
    index: usize,
    cx: &mut Context<MultisoulApp>,
) -> Option<Div> {
    use super::overview_brain::{pill, PillStyle};
    let (area_id, node_id, run_id, request_id) = decision_target(row)?;
    let make =
        |tag: &'static str, label: &'static str, allow: bool, cx: &mut Context<MultisoulApp>| {
            let (area_id, run_id, request_id) =
                (area_id.clone(), run_id.clone(), request_id.clone());
            pill(
                theme,
                ElementId::Name(format!("overview-{tag}-{index}").into()),
                label,
                if allow {
                    PillStyle::Primary
                } else {
                    PillStyle::Quiet
                },
                cx,
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.decide_overview_approval(&area_id, node_id, &run_id, &request_id, allow, cx);
            }))
        };
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(make("allow", ui::i18n::text("drawers.allow"), true, cx))
            .child(make("deny", ui::i18n::text("drawers.deny"), false, cx)),
    )
}
