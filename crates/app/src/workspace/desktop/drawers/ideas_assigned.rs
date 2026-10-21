//! 想法清单里“已指派”行：执行者、任务阶段与失败重试。

use super::super::ideas_rows::{AssignedRow, IdeaStatus};
use super::super::overview_rows::{OverviewFeedback, TONE_RUNNING};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::{Sizable, StyledExt};

const COPY_SECONDARY: u32 = 0xa9c0d5ff;

impl MultisoulApp {
    /// 分组标题：点击折叠/展开，右侧“清空”把已指派收进已完成。
    pub(super) fn assigned_heading(
        &self,
        count: usize,
        collapsed: bool,
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        div()
            .id("desktop-ideas-assigned-heading")
            .debug_selector(|| "desktop-ideas-assigned-heading".to_string())
            .h(px(30.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.desktop_workspace.ideas_assigned_collapsed =
                    !app.desktop_workspace.ideas_assigned_collapsed;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(11.0))
                    .font_semibold()
                    .text_color(rgba(0x9db7cdff))
                    .child(super::overview_parts::group_chevron(collapsed))
                    .child("已指派")
                    .child(
                        div()
                            .min_w(px(18.0))
                            .h(px(18.0))
                            .px(px(5.0))
                            .rounded(px(9.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgba(0x8fcaff2e))
                            .text_color(rgba(0xd4eaffff))
                            .child(count.to_string()),
                    ),
            )
            .child(
                Button::new("desktop-ideas-assigned-clear")
                    .ghost()
                    .small()
                    .tooltip("清空已指派（收进已完成）")
                    .child("清空")
                    .on_click(cx.listener(|app, _, _, cx| {
                        cx.stop_propagation();
                        app.clear_assigned_ideas(cx);
                    })),
            )
    }

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
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .flex_none()
                    .rounded_full()
                    .bg(rgba(TONE_RUNNING)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgba(0xc5d9eaff))
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
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
