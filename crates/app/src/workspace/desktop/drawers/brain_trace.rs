//! 大脑活动窗口：⌘⇧B 弹出，按“轮”显示大脑与 ACP 适配器的全部往来。
//!
//! 参考的设计：Apple 的内容优先与直接操作——一张卡片就是一轮，点标题展开，不弹二级菜单；
//! 窗口只读，Esc 或点空白处关闭；没有动效，打开即显示，减少动态效果的偏好天然满足。

use crate::app::{MultisoulApp, TurnCard};
use brain::acp::trace::{TraceItem, TraceKind};
use gpui::{
    div, prelude::*, px, rgba, ClipboardItem, Context, Div, ElementId, MouseButton, Stateful,
};
use std::time::SystemTime;
use ui::component::StyledExt;

/// 窗口里最多显示的轮数，旧的不渲染，避免上千条记录拖慢界面。
const MAX_CARDS: usize = 40;
/// 单条正文最多显示的字数；复制时仍然是全文。
const MAX_SHOWN_CHARS: usize = 4000;

impl MultisoulApp {
    fn area_name(&self, area_id: &str) -> String {
        self.desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .map(|area| area.display_name.clone())
            .unwrap_or_else(|| ui::i18n::text("chrome.brain_trace_this_project").to_string())
    }

    pub(crate) fn render_brain_trace(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let focus = self.brain_trace_focus(cx);
        let cards = self.brain_trace_cards(cx);
        let total = cards.len();
        let shown: Vec<TurnCard> = cards.into_iter().take(MAX_CARDS).collect();
        let body = if shown.is_empty() {
            div()
                .id("brain-trace-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(40.0))
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text("chrome.brain_trace_empty"))
        } else {
            let mut list = div()
                .id("brain-trace-scroll")
                .debug_selector(|| "brain-trace-scroll".to_string())
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(14.0));
            for card in shown {
                list = list.child(self.render_trace_card(card, cx));
            }
            list
        };
        div()
            .id("brain-trace-window")
            .debug_selector(|| "brain-trace-window".to_string())
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000091))
            .track_focus(&focus)
            .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    app.close_brain_trace(cx);
                    window.focus(&app.focus_handle);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    app.close_brain_trace(cx);
                    window.focus(&app.focus_handle);
                }),
            )
            .child(
                div()
                    .id("brain-trace-panel")
                    .w(px(880.0))
                    .max_w_full()
                    .h(px(640.0))
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .rounded(px(16.0))
                    // 玻璃质感的面板带透明度，长文本窗口需要近乎不透明才读得清。
                    .bg(rgba((theme.panel_high & 0xffffff00) | 0xf4))
                    .text_color(rgba(theme.text_primary))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(18.0))
                            .py(px(14.0))
                            .child(
                                div()
                                    .text_size(px(17.0))
                                    .font_semibold()
                                    .child(ui::i18n::text("chrome.brain_trace_title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(ui::i18n::format(
                                        "chrome.brain_trace_count",
                                        &[("n", total.to_string())],
                                    )),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child(ui::i18n::text("chrome.brain_trace_hint")),
                            ),
                    )
                    .child(body),
            )
    }

    fn render_trace_card(&mut self, card: TurnCard, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let key = (card.area.clone(), card.lane.clone(), card.turn);
        let expanded = self.trace_card_expanded(&key);
        let project = self.area_name(&card.area);
        let lane = ui::i18n::text(if card.lane == "Approval" {
            "chrome.brain_trace_lane_approval"
        } else {
            "chrome.brain_trace_lane_main"
        });
        let summary = card_summary(&card.items);
        let copy_text = card_plain_text(&card);
        let id = format!("brain-trace-card-{}-{}-{}", card.area, card.lane, card.turn);
        let header_key = key.clone();
        let mut chips = div().flex().items_center().gap(px(4.0));
        for (kind, count) in kind_counts(&card.items) {
            chips = chips.child(
                div()
                    .px(px(6.0))
                    .py(px(1.0))
                    .rounded(px(5.0))
                    .bg(rgba(kind_tint(kind)))
                    .text_size(px(11.0))
                    .child(format!("{} {count}", kind_label(kind))),
            );
        }
        let mut out = div()
            .id(ElementId::Name(id.into()))
            .flex_none()
            .rounded(px(10.0))
            .bg(rgba(0xffffff0a))
            .child(
                div()
                    .id(ElementId::Name(format!("{}-head", key.2).into()))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(10.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff12)))
                    .active(|style| style.bg(rgba(0xffffff1c)))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.toggle_trace_card(header_key.clone(), cx)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().font_semibold().child(project))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgba(theme.text_secondary))
                                    .child(format!(
                                        "{lane} · {} · {}",
                                        ui::i18n::format(
                                            "chrome.brain_trace_turn",
                                            &[("n", card.turn.to_string())]
                                        ),
                                        ago(card.at)
                                    )),
                            )
                            .child(div().flex_1())
                            .child(chips),
                    )
                    .when(!expanded, |head| {
                        head.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_tertiary))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(summary),
                        )
                    }),
            );
        if expanded {
            let mut items = div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .px(px(12.0))
                .pb(px(12.0));
            for item in &card.items {
                items = items.child(render_item(item, theme.text_secondary));
            }
            out = out.child(items).child(
                div().flex().justify_end().px(px(12.0)).pb(px(10.0)).child(
                    div()
                        .id(ElementId::Name(format!("{}-copy", key.2).into()))
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(7.0))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .bg(rgba(0xffffff14))
                        .hover(|style| style.bg(rgba(0xffffff26)))
                        .active(|style| style.bg(rgba(0xffffff36)))
                        .child(ui::i18n::text("chrome.brain_trace_copy"))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                        })),
                ),
            );
        }
        out
    }
}

fn render_item(item: &TraceItem, secondary: u32) -> Div {
    let shown: String = item.text.chars().take(MAX_SHOWN_CHARS).collect();
    let truncated = item.text.chars().count() > MAX_SHOWN_CHARS;
    div()
        .flex()
        .gap(px(10.0))
        .items_start()
        .child(
            div()
                .flex_none()
                .w(px(44.0))
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(5.0))
                .bg(rgba(kind_tint(item.kind)))
                .text_size(px(11.0))
                .child(kind_label(item.kind)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(!item.title.is_empty(), |col| {
                    col.child(
                        div()
                            .text_size(px(12.0))
                            .font_semibold()
                            .text_color(rgba(secondary))
                            .child(item.title.clone()),
                    )
                })
                .when(!shown.is_empty(), |col| {
                    col.child(div().text_size(px(12.5)).child(shown))
                })
                .when(truncated, |col| {
                    col.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(secondary))
                            .child(ui::i18n::text("chrome.brain_trace_truncated")),
                    )
                }),
        )
}

fn kind_label(kind: TraceKind) -> String {
    let key = match kind {
        TraceKind::Prompt => "chrome.brain_trace_kind_prompt",
        TraceKind::Reply => "chrome.brain_trace_kind_reply",
        TraceKind::Thought => "chrome.brain_trace_kind_thought",
        TraceKind::ToolCall => "chrome.brain_trace_kind_tool_call",
        TraceKind::ToolResult => "chrome.brain_trace_kind_tool_result",
        TraceKind::Plan => "chrome.brain_trace_kind_plan",
        TraceKind::Permission => "chrome.brain_trace_kind_permission",
        TraceKind::Turn => "chrome.brain_trace_kind_turn",
        TraceKind::Error => "chrome.brain_trace_kind_error",
        TraceKind::Lifecycle => "chrome.brain_trace_kind_lifecycle",
        TraceKind::Unknown => "chrome.brain_trace_kind_unknown",
    };
    ui::i18n::text(key).to_string()
}

fn kind_tint(kind: TraceKind) -> u32 {
    match kind {
        TraceKind::Prompt => 0x58adff33,
        TraceKind::Reply => 0xcde8ff22,
        TraceKind::Thought => 0xa78bfa33,
        TraceKind::ToolCall => 0xfb923c33,
        TraceKind::ToolResult => 0x4ade8033,
        TraceKind::Plan => 0x22d3ee33,
        TraceKind::Permission => 0xfacc1533,
        TraceKind::Error => 0xf8717144,
        TraceKind::Turn | TraceKind::Lifecycle | TraceKind::Unknown => 0xffffff18,
    }
}

/// 卡片里各种类的条数，按首次出现的顺序。
fn kind_counts(items: &[TraceItem]) -> Vec<(TraceKind, usize)> {
    let mut counts: Vec<(TraceKind, usize)> = Vec::new();
    for item in items {
        match counts.iter_mut().find(|(kind, _)| *kind == item.kind) {
            Some((_, count)) => *count += 1,
            None => counts.push((item.kind, 1)),
        }
    }
    counts
}

/// 折叠时的一行摘要：优先显示输出，其次输入。
fn card_summary(items: &[TraceItem]) -> String {
    let pick = |kind: TraceKind| {
        items
            .iter()
            .rev()
            .find(|i| i.kind == kind && !i.text.is_empty())
    };
    let item = pick(TraceKind::Reply).or_else(|| pick(TraceKind::Prompt));
    item.map(|i| i.text.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// 复制用的全文：每条一段，带种类和标题。
fn card_plain_text(card: &TurnCard) -> String {
    card.items
        .iter()
        .map(|item| format!("[{}] {}\n{}", kind_label(item.kind), item.title, item.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn ago(at: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(at)
        .map_or(0, |d| d.as_secs());
    ui::i18n::text(match seconds {
        0..=59 => "chrome.brain_trace_just_now",
        _ => "chrome.brain_trace_earlier",
    })
    .to_string()
}
