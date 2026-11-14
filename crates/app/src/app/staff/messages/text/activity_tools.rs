use super::super::*;
use crate::app::staff::activity::{TaskActivity, ToolStatus};
use gpui::{rgba, Div};
use ui::component::{
    button::{Button, ButtonVariants},
    Icon, IconName, Sizable,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(in crate::app::staff::messages::text) fn staff_tool_rows(
        &self,
        task_id: &str,
        activity: &TaskActivity,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let theme = self.staff_theme();
        activity
            .tools
            .iter()
            .enumerate()
            .map(|(index, tool)| {
                let key = format!("tool-{task_id}-{}-{}", activity.turn, tool.id);
                let expanded = self.staff.expanded.contains(&key);
                let (label, color, icon) = match tool.status {
                    ToolStatus::Running => {
                        ("staff.step_running", theme.text_secondary, IconName::Minus)
                    }
                    ToolStatus::Completed => {
                        ("staff.step_completed", theme.success, IconName::Check)
                    }
                    ToolStatus::Failed => (
                        "staff.activity_failed",
                        theme.destructive,
                        IconName::TriangleAlert,
                    ),
                    ToolStatus::Denied => (
                        "staff.activity_denied",
                        theme.accent_warm,
                        IconName::TriangleAlert,
                    ),
                    ToolStatus::Interrupted => (
                        "staff.activity_interrupted",
                        theme.text_secondary,
                        IconName::Minus,
                    ),
                };
                let mut row = div()
                    .flex_none()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(tokens::SPACE_4))
                    .child(
                        Button::new(("staff-tool", index))
                            .debug_selector(move || format!("staff-tool-{index}"))
                            .custom(self.staff_button_style(false, cx))
                            .small()
                            .w_full()
                            .h_auto()
                            .min_h(px(tokens::BUTTON_HEIGHT))
                            .justify_start()
                            .px(px(tokens::SPACE_8))
                            .py(px(tokens::SPACE_4))
                            .child(
                                Icon::new(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .with_size(px(tokens::LABEL_TEXT)),
                            )
                            .child(
                                Icon::new(icon)
                                    .with_size(px(tokens::LABEL_TEXT))
                                    .text_color(rgba(color)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_left()
                                    .child(truncate(&tool.name, 100)),
                            )
                            .child(
                                div()
                                    .text_color(rgba(color))
                                    .text_size(px(tokens::META_TEXT))
                                    .child(t(if tool.command_format_error {
                                        "staff.command_format_error"
                                    } else {
                                        label
                                    })),
                            )
                            .when_some(tool.elapsed_ms, |button, elapsed| {
                                button.child(div().text_size(px(tokens::META_TEXT)).child(
                                    ui::i18n::format(
                                        "staff.activity_duration",
                                        &[("seconds", format!("{:.1}", elapsed as f64 / 1000.))],
                                    ),
                                ))
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if !app.staff.expanded.remove(&key) {
                                    app.staff.expanded.insert(key.clone());
                                }
                                app.notify_staff(cx);
                            })),
                    );
                if expanded {
                    row = row.child(
                        div()
                            .id(("staff-tool-detail", index))
                            .debug_selector(move || format!("staff-tool-detail-{index}"))
                            .flex()
                            .flex_col()
                            .gap(px(tokens::SPACE_8))
                            .px(px(tokens::SPACE_16))
                            .py(px(tokens::SPACE_8))
                            .rounded(px(tokens::SPACE_8))
                            .bg(rgba(theme.panel))
                            .text_size(px(tokens::LABEL_TEXT))
                            .when(!tool.input.is_empty(), |body| {
                                body.child(
                                    div()
                                        .text_color(rgba(theme.text_secondary))
                                        .child(truncate(&tool.input, 320)),
                                )
                            })
                            .when(!tool.output.is_empty(), |body| {
                                body.child(div().child(if tool.command_format_error {
                                    t("staff.command_format_detail").to_string()
                                } else {
                                    tool.output.clone()
                                }))
                            }),
                    );
                }
                row
            })
            .collect()
    }
}
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(limit).collect::<String>())
    }
}
