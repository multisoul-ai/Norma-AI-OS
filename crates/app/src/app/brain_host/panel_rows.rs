//! 幕僚活动面板里的一行：时间列加内容列，轮次与动作两种样子。

use super::activity_feed::{clock, relative, FeedAction, FeedRow, FeedTurn};
use super::activity_view::{row_key, RowKey};
use super::log::ASSIGN_KEY;
use super::panel_parts::{
    badge, duration_label, kind_label, kind_tint, pill, plain_text, relative_label, render_item,
    trigger_label,
};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClipboardItem, Context, Div, ElementId, Stateful};
use std::time::Instant;
use ui::component::StyledExt;

const BRAIN_TINT: u32 = 0xa68bff;
const WARN: u32 = 0xffb15c;

impl MultisoulApp {
    pub(super) fn render_feed_row(
        &mut self,
        row: FeedRow,
        now_ms: i64,
        offset: i64,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let key = row_key(&row);
        let expanded = self.activity().is_expanded(&key);
        let flashing = self.activity().flashing(&key, now);
        let at_ms = row.at_ms();
        let time = div()
            .flex_none()
            .w(px(70.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family("Menlo")
                    .text_size(px(12.0))
                    .child(clock(at_ms, offset)),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgba(theme.text_tertiary))
                    .child(relative_label(relative(now_ms, at_ms))),
            );
        let (content, accent) = match &row {
            FeedRow::Turn(turn) => (
                self.render_turn_content(turn, expanded, cx),
                if turn.needs_you { WARN } else { 0 },
            ),
            FeedRow::Action(action) => {
                (self.render_action_content(action, expanded, cx), BRAIN_TINT)
            }
        };
        let id = match &key {
            RowKey::Turn(k) => format!("brain-feed-turn-{}-{}-{}-{}", k.0, k.1, k.2, k.3),
            RowKey::Action(k) => format!("brain-feed-action-{}-{}-{}", k.0, k.1, k.2),
        };
        let toggle_key = key.clone();
        div()
            .id(ElementId::Name(id.into()))
            .flex_none()
            .flex()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(9.0))
            .rounded(px(10.0))
            .bg(rgba(0xffffff08))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0xffffff12)))
            .active(|s| s.bg(rgba(0xffffff1c)))
            // 所有行都留 3px 左边框，没有强调色的行用透明色，时间列才能上下对齐。
            .border_l(px(3.0))
            .border_color(rgba(if accent != 0 { (accent << 8) | 0xff } else { 0 }))
            .when(flashing, |d| {
                d.border_2().border_color(rgba((BRAIN_TINT << 8) | 0xff))
            })
            .on_click(cx.listener(move |app, _, _, cx| {
                app.toggle_feed_row(toggle_key.clone(), cx);
            }))
            .child(time)
            .child(content)
    }

    fn render_turn_content(
        &mut self,
        turn: &FeedTurn,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let lane = ui::i18n::text(if turn.key.2 == "Approval" {
            "chrome.brain_trace_lane_approval"
        } else {
            "chrome.brain_trace_lane_main"
        });
        let meta = match turn.finished_ms {
            _ if turn.is_session_event() => ui::i18n::format(
                "chrome.brain_feed_meta_session",
                &[("lane", lane.to_string())],
            ),
            Some(end) => ui::i18n::format(
                "chrome.brain_feed_meta",
                &[
                    ("lane", lane.to_string()),
                    ("n", turn.key.3.to_string()),
                    ("dur", duration_label(end - turn.started_ms)),
                ],
            ),
            None => ui::i18n::format(
                "chrome.brain_feed_meta_running",
                &[("lane", lane.to_string()), ("n", turn.key.3.to_string())],
            ),
        };
        let trigger = ui::i18n::format(
            "chrome.brain_feed_trigger_label",
            &[("what", trigger_label(turn.trigger, &turn.trigger_who))],
        );
        let mut chips = div().flex().items_center().gap(px(4.0)).flex_none();
        for (kind, count) in &turn.chips {
            let kind = super::activity_sink::kind_from_key(kind);
            chips = chips.child(badge(
                format!("{} {count}", kind_label(kind)),
                kind_tint(kind),
            ));
        }
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().font_semibold().child(self.area_name(&turn.key.1)))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(meta),
            )
            .when(!turn.is_session_event(), |d| {
                d.child(badge(trigger, 0xffffff14))
            })
            .when(turn.needs_you, |d| {
                d.child(badge(
                    ui::i18n::text("chrome.brain_feed_needs_badge").to_string(),
                    0xffb15c33,
                ))
            })
            .when(turn.produced > 0, |d| {
                d.child(badge(
                    ui::i18n::format(
                        "chrome.brain_feed_produced",
                        &[("n", turn.produced.to_string())],
                    ),
                    0xa68bff2e,
                ))
            })
            .child(div().flex_1())
            .child(chips);
        let mut out = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(head);
        if !expanded {
            return out.child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba(theme.text_secondary))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        turn.summary
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
            );
        }
        let items = self.activity().details.get(&turn.key).cloned();
        let Some(items) = items else {
            return out.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_tertiary))
                    .child(ui::i18n::text("chrome.brain_feed_loading")),
            );
        };
        let copy = plain_text(&items);
        out = out.child(
            div().flex().flex_col().gap(px(8.0)).pt(px(4.0)).children(
                items
                    .iter()
                    .map(|item| render_item(item, theme.text_secondary)),
            ),
        );
        out.child(
            div().flex().justify_end().child(
                pill(
                    ElementId::Name(
                        format!("brain-feed-copy-{}-{}", turn.key.0, turn.key.3).into(),
                    ),
                    ui::i18n::text("chrome.brain_trace_copy"),
                    false,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                })),
            ),
        )
    }

    fn render_action_content(
        &mut self,
        action: &FeedAction,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let row = &action.row;
        let (verb, tint) = match row.kind.as_str() {
            "approved" => ("chrome.brain_feed_verb_approved", 0x6fdc8c26),
            "denied" => ("chrome.brain_feed_verb_denied", 0xff7a7a26),
            "user" => ("chrome.brain_feed_verb_user", 0x58adff2e),
            _ => ("chrome.brain_feed_verb_assigned", 0xa68bff2e),
        };
        let trigger = ui::i18n::format(
            "chrome.brain_feed_trigger_label",
            &[("what", trigger_label(action.trigger, &action.trigger_who))],
        );
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(badge(ui::i18n::text(verb).to_string(), tint))
            .child(div().font_semibold().child(self.area_name(&row.area_id)))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(row.agent.clone()),
            )
            .child(badge(trigger, 0xffffff14));
        let mut out = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(head)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba(theme.text_secondary))
                    .child(row.summary.clone()),
            );
        if !expanded {
            return out;
        }
        let id = format!("{}-{}", row.launch_id, row.at_ms);
        let mut buttons = div().flex().flex_wrap().gap(px(6.0));
        if row.kind != "user" {
            let key = row.ask_key.clone();
            buttons = buttons.child(
                pill(
                    ElementId::Name(format!("brain-feed-ask-{id}").into()),
                    ui::i18n::text("chrome.brain_feed_ask_first"),
                    false,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.brain_ask_first(key.clone(), cx);
                })),
            );
        }
        let (area, node) = (row.area_id.clone(), row.node_id as u64);
        let mut stop = pill(
            ElementId::Name(format!("brain-feed-stop-{id}").into()),
            ui::i18n::text("chrome.brain_feed_stop_agent"),
            false,
        );
        if node == 0 {
            stop = stop.opacity(0.4);
        } else {
            stop = stop.on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.brain_stop_agent(&area, node, cx);
            }));
        }
        buttons = buttons.child(stop);
        if let Some(source) = action.source.clone() {
            buttons = buttons.child(
                div()
                    .id(ElementId::Name(format!("brain-feed-source-{id}").into()))
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(rgba(0x58adffff))
                    .hover(|s| s.bg(rgba(0x58adff22)))
                    .active(|s| s.bg(rgba(0x58adff38)))
                    .child(ui::i18n::format(
                        "chrome.brain_feed_from_turn",
                        &[("n", source.3.to_string())],
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.jump_to_feed_turn(&source, cx);
                    })),
            );
        }
        out = out
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(ui::i18n::format(
                        "chrome.brain_reason",
                        &[("reason", row.reason.clone())],
                    )),
            )
            .child(buttons);
        out
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
}
