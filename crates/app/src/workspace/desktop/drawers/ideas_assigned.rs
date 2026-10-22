//! 想法清单里“已指派”行：执行者、任务阶段与失败重试。

use super::super::ideas_rows::{AssignedRow, IdeaStatus};
use super::super::overview_rows::{OverviewFeedback, TONE_RUNNING};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::Sizable;

const COPY_SECONDARY: u32 = 0xa9c0d5ff;

impl MultisoulApp {
    pub(super) fn render_assigned_row(
        &self,
        row: &AssignedRow,
        focused: bool,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let element_id = format!("desktop-idea-assigned-{}", row.id);
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let locate = row.area_id.clone().zip(row.node_id);
        let retry = row.retryable.then(|| row.id.clone());
        let status = match &row.status {
            IdeaStatus::Sending => "投递中…".to_owned(),
            IdeaStatus::AssignedTo { name, phase } => match phase_label(*phase) {
                Some(label) => format!("已交给 {name} · {label}"),
                None => format!("已交给 {name}"),
            },
        };
        div()
            .id(ElementId::Name(element_id.clone().into()))
            .debug_selector(move || element_id.clone())
            .w_full()
            .min_h(px(48.0))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(if focused { feedback.focus } else { 0x00000000 }))
            .when(locate.is_some(), |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(rgba(hover)))
                    .active(move |style| style.bg(rgba(pressed)))
            })
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                if let Some((area_id, node_id)) = locate.clone() {
                    app.locate_desktop_agent(&area_id, node_id, window, cx);
                }
            }))
            .child(div().w(px(14.0)).flex_none())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .flex_none()
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgba(ui::icons::with_alpha(TONE_RUNNING, 0x29)))
                                    .child(
                                        div()
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .rounded_full()
                                            .bg(rgba(TONE_RUNNING)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(rgba(0xc5d9eaff))
                                    .child(row.title.clone()),
                            ),
                    )
                    .child(
                        div()
                            .pl(px(22.0))
                            .text_size(px(12.0))
                            .text_color(rgba(COPY_SECONDARY))
                            .child(status),
                    ),
            )
            .when_some(retry, |row, id| {
                row.child(
                    Button::new(ElementId::Name(format!("idea-retry-{id}").into()))
                        .ghost()
                        .small()
                        .tooltip("退回待办，重新指派")
                        .child("重试")
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.retry_idea(&id, cx);
                        })),
                )
            })
    }
}

fn phase_label(phase: Option<workspace::ideas::TaskPhase>) -> Option<&'static str> {
    use workspace::ideas::TaskPhase;
    match phase? {
        TaskPhase::Running => Some("运行中"),
        TaskPhase::Queued => Some("已排队"),
        TaskPhase::Failed => Some("失败"),
        TaskPhase::Succeeded => Some("已完成"),
        TaskPhase::Cancelled => Some("已取消"),
        TaskPhase::Untracked => Some("已发送，无法追踪完成"),
    }
}
