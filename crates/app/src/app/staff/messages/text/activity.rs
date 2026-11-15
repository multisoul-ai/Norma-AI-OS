use super::super::*;
use crate::app::staff::activity::{card_key, TaskActivity, TimelineEntry};
use gpui::{div, px, relative, rgba, Div};
use ui::component::{spinner::Spinner, Icon, IconName, Sizable};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(in crate::app::staff) fn staff_activity_view(
        &self,
        task: &Task,
        activity: &TaskActivity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let task_id = task.id.as_str();
        let theme = self.staff_theme();
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_12))
            .text_size(px(tokens::BODY_TEXT));
        if activity.waiting()
            || !activity.reply.is_empty()
            || activity.failed
            || !activity.tools.is_empty()
            || !activity.timeline.is_empty()
        {
            let status = if activity.failed && !activity.waiting() {
                "staff.response_failed"
            } else if !activity.waiting() {
                "staff.activity_completed"
            } else if !activity.active {
                "staff.response_queued"
            } else if activity.working() {
                "staff.response_working"
            } else if activity.reply.is_empty() {
                "staff.response_waiting"
            } else {
                "staff.response_streaming"
            };
            // Same two rows as Prose: byline first, then the status line under it (prototype `.activity`).
            let byline = super::staff_byline(self);
            let mut header = div()
                .id("staff-response-status")
                .debug_selector(|| "staff-response-status".into())
                .flex()
                .items_center()
                .gap(px(tokens::SPACE_8))
                .text_size(px(tokens::LABEL_TEXT))
                .text_color(rgba(theme.text_secondary));
            header = if activity.waiting() && self.motion.reduce_motion {
                header.child(Icon::new(IconName::Loader).with_size(px(tokens::BODY_TEXT)))
            } else if activity.waiting() {
                header.child(
                    Spinner::new()
                        .with_size(px(tokens::BODY_TEXT))
                        .color(rgba(theme.accent_cyan).into()),
                )
            } else {
                header.child(
                    Icon::new(if activity.failed {
                        IconName::TriangleAlert
                    } else {
                        IconName::Check
                    })
                    .with_size(px(tokens::BODY_TEXT))
                    .text_color(rgba(if activity.failed {
                        theme.accent_warm
                    } else {
                        theme.success
                    })),
                )
            };
            // No steps yet: steps and duration would read "0 步 · 0.0 秒", so show the status alone.
            let summary = if activity.tools.is_empty() {
                t(status).to_owned()
            } else {
                ui::i18n::format(
                    "staff.activity_summary",
                    &[
                        ("status", t(status).to_owned()),
                        ("steps", activity.tools.len().to_string()),
                        (
                            "seconds",
                            format!("{:.1}", activity.duration_ms() as f64 / 1000.),
                        ),
                    ],
                )
            };
            body = body.child(byline).child(header.child(summary));
            // 只追加：条目按到达顺序排列，卡片入库后新的文字和工具都排在它下面。
            let last_text = activity
                .timeline
                .iter()
                .rposition(|entry| matches!(entry, TimelineEntry::Text(_)));
            for (position, entry) in activity.timeline.iter().enumerate() {
                body = match entry {
                    TimelineEntry::Tool(index) => {
                        body.child(self.staff_tool_row(task_id, activity, *index, cx))
                    }
                    TimelineEntry::Card(key) => match task
                        .messages
                        .iter()
                        .position(|message| card_key(message).as_ref() == Some(key))
                    {
                        Some(index) => body.child(self.render_message(
                            task,
                            index,
                            &task.messages[index],
                            window,
                            cx,
                        )),
                        None => body,
                    },
                    TimelineEntry::Text(text) => body.child(
                        div()
                            .id(("staff-streaming-reply", position))
                            .when(last_text == Some(position), |text| {
                                text.debug_selector(|| "staff-streaming-reply".into())
                            })
                            .line_height(relative(tokens::BODY_LEADING))
                            .child(text.clone()),
                    ),
                };
            }
        }
        if !activity.plan.is_empty() {
            let completed = activity
                .plan
                .iter()
                .filter(|(status, _)| status == "completed")
                .count();
            let mut plan = div()
                .id("staff-plan")
                .debug_selector(|| "staff-plan".into())
                .border_1()
                .border_color(rgba(self.staff_line()))
                .rounded(px(tokens::SPACE_12))
                .child(
                    div()
                        .px(px(tokens::SPACE_12))
                        .py(px(tokens::SPACE_8))
                        .text_size(px(tokens::SPACE_12))
                        .text_color(rgba(theme.text_secondary))
                        .child(format!(
                            "{}  {completed}/{}",
                            t("staff.execution_plan"),
                            activity.plan.len()
                        )),
                );
            for (status, text) in &activity.plan {
                let color = if status == "completed" {
                    theme.success
                } else if status == "in_progress" {
                    theme.accent_cyan
                } else {
                    theme.text_tertiary
                };
                plan = plan.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(tokens::SPACE_12))
                        .px(px(tokens::SPACE_12))
                        .py(px(tokens::SPACE_8))
                        .border_t_1()
                        .border_color(rgba(self.staff_line()))
                        .text_size(px(tokens::LABEL_TEXT))
                        .child(
                            Icon::new(if status == "completed" {
                                IconName::Check
                            } else if status == "in_progress" {
                                IconName::Loader
                            } else {
                                IconName::Minus
                            })
                            .with_size(px(tokens::BODY_TEXT))
                            .text_color(rgba(color)),
                        )
                        .child(div().flex_1().min_w_0().child(text.clone()))
                        .child(
                            div()
                                .text_size(px(tokens::SPACE_12))
                                .text_color(rgba(color))
                                .child(t(match status.as_str() {
                                    "completed" => "staff.step_completed",
                                    "in_progress" => "staff.step_running",
                                    _ => "staff.step_pending",
                                })),
                        ),
                );
            }
            body = body.child(plan);
        }
        body
    }
}
