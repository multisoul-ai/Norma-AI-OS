//! 幕僚活动面板：⌘⇧B 弹出，按时间倒序统一列出大脑的每一轮与自主处理。
//!
//! 参考的设计：Apple 的内容优先与直接操作——一行就是一件事，点行展开，不弹二级菜单；
//! 面板只读（除了动作行上的两个管理按钮），Esc 或点空白处关闭。
//! 动效：没有。跳转后的高亮是一圈 1 秒的静态描边，减少动态效果的偏好天然满足。

use super::activity_feed::{
    apply_filter, day_bucket, feed_counts, local_offset_secs, DayBucket, FeedKind, FeedRow,
};
use super::activity_sink::unix_ms;
use super::activity_view::{row_key, RowKey};
use super::panel_parts::{day_label, pill};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, MouseButton, Stateful};
use std::time::{Instant, SystemTime};
use ui::component::StyledExt;

enum Entry {
    Day(String),
    Row(Box<FeedRow>),
}

impl MultisoulApp {
    pub(crate) fn render_brain_trace(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let focus = self.brain_trace_focus(cx);
        let now_ms = unix_ms(SystemTime::now());
        let offset = local_offset_secs(now_ms);
        let activity = self.activity();
        let counts = feed_counts(&activity.rows);
        let visible: Vec<FeedRow> = apply_filter(&activity.rows, &activity.filter)
            .into_iter()
            .cloned()
            .collect();
        let filter = activity.filter.clone();
        let more = activity.may_have_more;
        let mut entries: Vec<Entry> = Vec::new();
        let mut last: Option<DayBucket> = None;
        for row in visible {
            let bucket = day_bucket(now_ms, row.at_ms(), offset);
            if last != Some(bucket) {
                entries.push(Entry::Day(day_label(bucket)));
                last = Some(bucket);
            }
            entries.push(Entry::Row(Box::new(row)));
        }
        if let Some(target) = self.activity().scroll_target() {
            let index = entries
                .iter()
                .position(|e| matches!(e, Entry::Row(row) if row_key(row) == target));
            if let Some(index) = index {
                self.activity().scroll.scroll_to_top_of_item(index);
            }
            self.activity_mut().scroll_to = None;
        }
        let now = Instant::now();
        let body = if entries.is_empty() {
            let empty = if counts.all == 0 {
                "chrome.brain_trace_empty"
            } else {
                "chrome.brain_feed_no_match"
            };
            div()
                .id("brain-trace-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(40.0))
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text(empty))
                .into_any_element()
        } else {
            let mut list = div()
                .id("brain-trace-scroll")
                .debug_selector(|| "brain-trace-scroll".to_string())
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(&self.activity().scroll)
                .flex()
                .flex_col()
                .gap(px(6.0))
                .px(px(14.0))
                .pb(px(16.0));
            for entry in entries {
                list = match entry {
                    Entry::Day(label) => list.child(
                        div()
                            .flex_none()
                            .pt(px(8.0))
                            .pb(px(2.0))
                            .px(px(4.0))
                            .text_size(px(11.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child(label),
                    ),
                    Entry::Row(row) => {
                        list.child(self.render_feed_row(*row, now_ms, offset, now, cx))
                    }
                };
            }
            if more {
                list = list.child(
                    pill(
                        "brain-feed-more",
                        ui::i18n::text("chrome.brain_feed_more"),
                        false,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.activity_mut().load_more();
                        app.refresh_brain_activity(true, cx);
                    })),
                );
            }
            list.into_any_element()
        };
        let tools = self.render_feed_tools(counts, &filter, cx);
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
                            .pt(px(14.0))
                            .pb(px(8.0))
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
                                        &[("n", counts.all.to_string())],
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
                    .child(tools)
                    .child(body),
            )
    }

    fn render_feed_tools(
        &mut self,
        counts: super::activity_feed::FeedCounts,
        filter: &super::activity_feed::FeedFilter,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let kinds = [
            (FeedKind::All, "chrome.brain_feed_all", counts.all),
            (
                FeedKind::Actions,
                "chrome.brain_feed_actions",
                counts.actions,
            ),
            (FeedKind::Turns, "chrome.brain_feed_turns", counts.turns),
            (FeedKind::Needs, "chrome.brain_feed_needs", counts.needs),
        ];
        let mut seg = div()
            .flex_none()
            .flex()
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(9.0))
            .bg(rgba(0xffffff0a));
        for (kind, key, count) in kinds {
            seg = seg.child(
                pill(
                    ElementId::Name(format!("brain-feed-kind-{key}").into()),
                    &format!("{} {count}", ui::i18n::text(key)),
                    filter.kind == kind,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.activity_mut().set_kind(kind);
                    cx.notify();
                })),
            );
        }
        let area = match &filter.area {
            Some(area) => self.area_name(area),
            None => ui::i18n::text("chrome.brain_feed_area_all").to_string(),
        };
        let trigger = match filter.trigger {
            Some(trigger) => super::panel_parts::trigger_label(trigger, ""),
            None => ui::i18n::text("chrome.brain_feed_trigger_all").to_string(),
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(18.0))
            .pb(px(10.0))
            .text_color(rgba(theme.text_secondary))
            .child(seg)
            .child(
                pill(
                    "brain-feed-area",
                    &format!("{area} ▾"),
                    filter.area.is_some(),
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    app.activity_mut().cycle_area();
                    cx.notify();
                })),
            )
            .child(
                pill(
                    "brain-feed-trigger",
                    &format!("{trigger} ▾"),
                    filter.trigger.is_some(),
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    app.activity_mut().cycle_trigger();
                    cx.notify();
                })),
            )
    }

    pub(super) fn area_name(&self, area_id: &str) -> String {
        self.desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .map(|area| area.display_name.clone())
            .unwrap_or_else(|| ui::i18n::text("chrome.brain_trace_this_project").to_string())
    }

    /// 点“来自第 N 轮”：清筛选、展开并滚到那一轮。
    pub(super) fn jump_to_feed_turn(
        &mut self,
        key: &super::activity_feed::TurnKey,
        cx: &mut Context<Self>,
    ) {
        if self.activity_mut().jump_to(key) {
            self.refresh_brain_activity(true, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_feed_row(&mut self, key: RowKey, cx: &mut Context<Self>) {
        self.activity_mut().toggle(key);
        self.refresh_brain_activity(true, cx);
        cx.notify();
    }
}
