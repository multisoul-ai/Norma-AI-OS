//! 幕僚活动面板的小零件：胶囊按钮、徽标、时间与触发源文案、正文条目。

use super::activity_feed::{DayBucket, FeedTrigger, Relative};
use brain::acp::trace::{TraceItem, TraceKind};
use brain::TriggerKind;
use gpui::{div, prelude::*, px, rgba, Div, ElementId, SharedString, Stateful};
use ui::component::StyledExt;

/// 单条正文最多显示的字数；复制时仍然是全文。
const MAX_SHOWN_CHARS: usize = 4000;

/// 小胶囊按钮：悬停、按下各一档，选中态更亮。
pub(super) fn pill(id: impl Into<ElementId>, label: &str, selected: bool) -> Stateful<Div> {
    let (fill, hover, pressed) = if selected {
        (0x58adff2e, 0x58adff44, 0x58adff5a)
    } else {
        (0xffffff0a, 0xffffff1c, 0xffffff2e)
    };
    div()
        .id(id)
        .flex_none()
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(7.0))
        .cursor_pointer()
        .text_size(px(12.0))
        .bg(rgba(fill))
        .hover(move |s| s.bg(rgba(hover)))
        .active(move |s| s.bg(rgba(pressed)))
        .child(SharedString::from(label.to_owned()))
}

pub(super) fn day_label(bucket: DayBucket) -> String {
    match bucket {
        DayBucket::Today => ui::i18n::text("chrome.brain_feed_today").to_string(),
        DayBucket::Yesterday => ui::i18n::text("chrome.brain_feed_yesterday").to_string(),
        DayBucket::Date { month, day } => ui::i18n::format(
            "chrome.brain_feed_date",
            &[("m", month.to_string()), ("d", day.to_string())],
        ),
    }
}

pub(super) fn relative_label(value: Relative) -> String {
    let (key, n) = match value {
        Relative::JustNow => return ui::i18n::text("chrome.brain_trace_just_now").to_string(),
        Relative::Minutes(n) => ("chrome.brain_feed_minutes_ago", n),
        Relative::Hours(n) => ("chrome.brain_feed_hours_ago", n),
        Relative::Days(n) => ("chrome.brain_feed_days_ago", n),
    };
    ui::i18n::format(key, &[("n", n.to_string())])
}

pub(super) fn trigger_label(trigger: FeedTrigger, who: &str) -> String {
    let key = match trigger {
        FeedTrigger::UserApproved => "chrome.brain_trigger_user_approved",
        FeedTrigger::Brain(TriggerKind::AgentFinished) => "chrome.brain_trigger_agent_finished",
        FeedTrigger::Brain(TriggerKind::ApprovalRequest) => "chrome.brain_trigger_approval_request",
        FeedTrigger::Brain(TriggerKind::UserRequest) => "chrome.brain_trigger_user_request",
        FeedTrigger::Brain(TriggerKind::UserReturned) => "chrome.brain_trigger_user_returned",
        FeedTrigger::Brain(TriggerKind::Scheduled) => "chrome.brain_trigger_scheduled",
        FeedTrigger::Brain(TriggerKind::Unknown) => "chrome.brain_trigger_unknown",
    };
    let name = ui::i18n::text(key);
    if who.is_empty() {
        name.to_string()
    } else {
        format!("{name}：{who}")
    }
}

pub(super) fn duration_label(ms: i64) -> String {
    let secs = (ms.max(0) / 1000) as u64;
    match secs {
        0 if ms < 1000 => ui::i18n::text("chrome.brain_feed_duration_short").to_string(),
        0..=59 => ui::i18n::format(
            "chrome.brain_feed_duration_secs",
            &[("n", secs.to_string())],
        ),
        _ => ui::i18n::format(
            "chrome.brain_feed_duration_mins",
            &[
                ("m", (secs / 60).to_string()),
                ("s", (secs % 60).to_string()),
            ],
        ),
    }
}

pub(super) fn kind_label(kind: TraceKind) -> &'static str {
    ui::i18n::text(match kind {
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
    })
}

pub(super) fn kind_tint(kind: TraceKind) -> u32 {
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

pub(super) fn badge(text: String, tint: u32) -> Div {
    div()
        .flex_none()
        .px(px(7.0))
        .py(px(1.0))
        .rounded(px(5.0))
        .bg(rgba(tint))
        .text_size(px(11.0))
        .child(text)
}

pub(super) fn render_item(item: &TraceItem, secondary: u32) -> Div {
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

pub(super) fn plain_text(items: &[TraceItem]) -> String {
    items
        .iter()
        .map(|item| format!("[{}] {}\n{}", kind_label(item.kind), item.title, item.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}
