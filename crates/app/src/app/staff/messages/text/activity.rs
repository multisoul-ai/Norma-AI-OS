use super::super::*;
use crate::app::staff::activity::TaskActivity;
use gpui::{div, px, relative, rgba, Div};
use ui::component::{spinner::Spinner, Icon, IconName, Sizable, StyledExt};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(in crate::app::staff) fn staff_activity_view(
        &self,
        task_id: &str,
        activity: &TaskActivity,
        cx: &mut Context<Self>,
    ) -> Div {
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
            let mut header = div()
                .id("staff-response-status")
                .debug_selector(|| "staff-response-status".into())
                .flex()
                .items_center()
                .gap(px(tokens::SPACE_8))
                .text_size(px(tokens::SPACE_12))
                .text_color(rgba(theme.text_secondary))
                .child(crate::app::chrome::brand_logo())
                .child(div().font_semibold().child("Norma"));
            if activity.waiting() {
                header = if self.motion.reduce_motion {
                    header.child(Icon::new(IconName::Loader).with_size(px(tokens::BODY_TEXT)))
                } else {
                    header.child(
                        Spinner::new()
                            .with_size(px(tokens::BODY_TEXT))
                            .color(rgba(theme.accent_cyan).into()),
                    )
                };
            }
            let summary = ui::i18n::format(
                "staff.activity_summary",
                &[
                    ("status", t(status).to_owned()),
                    ("steps", activity.tools.len().to_string()),
                    (
                        "seconds",
                        format!("{:.1}", activity.duration_ms() as f64 / 1000.),
                    ),
                ],
            );
            body = body.child(header.child(summary));
            body = body.children(self.staff_tool_rows(task_id, activity, cx));
            if !activity.reply.is_empty() {
                body = body.child(
                    div()
                        .id("staff-streaming-reply")
                        .debug_selector(|| "staff-streaming-reply".into())
                        .line_height(relative(tokens::BODY_LEADING))
                        .child(activity.reply.clone()),
                );
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
