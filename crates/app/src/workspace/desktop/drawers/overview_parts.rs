//! 概览行的组成部件：行首圆点、文案、状态角标、步骤条与操作文字。

use super::super::overview_rows::{
    OverviewAction, OverviewRow, OverviewStatusTone, ProgressMark, OVERVIEW_TEXT_COPY,
    OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY, TONE_RUNNING,
};
use gpui::{div, prelude::*, px, rgba, Div};
use ui::component::StyledExt;
use ui::icons::with_alpha;

const AMBER_TINT: u32 = 0xf2c77326;
const AMBER_TEXT: u32 = 0xf6dfaaff;
const COPY_PRIMARY: u32 = 0xeef6ffff;
const COPY_SECONDARY: u32 = 0xa9c0d5ff;
const COPY_STEP: u32 = 0xcfe3f4ff;
const TRAIL: u32 = 0x9fc7eaff;

pub(super) fn row_copy(row: &OverviewRow, color: u32, theme: ui::Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(3.0))
        .child(primary_line(row, color, theme))
        .when(
            row.task_line.is_some() || !row.secondary.is_empty(),
            |copy| {
                copy.child(
                    div()
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(COPY_SECONDARY))
                        .overflow_hidden()
                        .child(
                            row.task_line
                                .clone()
                                .unwrap_or_else(|| row.secondary.clone()),
                        ),
                )
            },
        )
        .when_some(row.progress.clone(), |copy, marks| {
            copy.child(progress_bar(&marks))
        })
        .when_some(row.detail.clone(), |copy, detail| {
            copy.child(
                div()
                    .mt(px(if row.progress.is_some() { 1.0 } else { 0.0 }))
                    .text_size(px(OVERVIEW_TEXT_COPY))
                    .text_color(rgba(COPY_STEP))
                    .child(detail),
            )
        })
}

pub(super) fn primary_line(row: &OverviewRow, color: u32, _theme: ui::Theme) -> Div {
    let agent_row = matches!(
        row.action,
        OverviewAction::LocateAgent { .. } | OverviewAction::StopEgo { .. }
    );
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .min_w(px(0.0))
        .child(
            div()
                .flex_shrink()
                .text_size(px(OVERVIEW_TEXT_PRIMARY))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(COPY_PRIMARY))
                .child(row.primary.clone()),
        )
        .when_some(
            row.status_label.clone().filter(|_| agent_row),
            |line, label| {
                let running = color == TONE_RUNNING;
                let (background, foreground) = if running {
                    (0x36dce81f, 0x7ee9eeff)
                } else {
                    (0x9fb8cf1a, 0xb3c6d8ff)
                };
                line.child(status_chip(&label, background, foreground))
            },
        )
        .when_some(row.approval_badge.clone(), |line, badge| {
            line.child(status_chip(&badge, AMBER_TINT, AMBER_TEXT))
        })
}

pub(super) fn status_chip(label: &str, background: u32, foreground: u32) -> Div {
    div()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(5.0))
        .flex()
        .items_center()
        .flex_shrink()
        .bg(rgba(background))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(foreground))
        .child(label.to_owned())
}

/// 行首 20px 圆点；项目行是 24px 的序号小框。待审核与运行中的点带 3px 光晕。
pub(super) fn row_lead(row: &OverviewRow, color: u32, agent_row: bool) -> Div {
    let lead = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when(agent_row, |lead| lead.mt(px(2.0)));
    if let Some(label) = row.lead_label.clone() {
        return lead.w(px(24.0)).h(px(20.0)).child(
            div()
                .w(px(24.0))
                .h(px(20.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgba(0xcde8ff33))
                .flex()
                .items_center()
                .justify_center()
                .font_family("Menlo")
                .text_size(px(OVERVIEW_TEXT_SECONDARY))
                .font_semibold()
                .text_color(rgba(0x9fb8cfff))
                .child(label),
        );
    }
    let halo = matches!(
        row.status_tone,
        OverviewStatusTone::Approval | OverviewStatusTone::Running
    );
    lead.w(px(20.0)).h(px(20.0)).child(
        div()
            .w(px(14.0))
            .h(px(14.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .when(halo, |ring| ring.bg(rgba(with_alpha(color, 0x29))))
            .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(rgba(color))),
    )
}

pub(super) fn action_label(label: &str, agent_row: bool) -> Div {
    div()
        .flex_none()
        .px(px(4.0))
        .when(agent_row, |action| action.mt(px(2.0)))
        .text_size(px(OVERVIEW_TEXT_COPY))
        .text_color(rgba(TRAIL))
        .child(format!("{label} ↗"))
}

/// 步骤条：已完成实心，当前一格左半实心右半淡色，其余为底色。
pub(super) fn progress_bar(marks: &[ProgressMark]) -> Div {
    div()
        .mt(px(5.0))
        .flex()
        .items_center()
        .gap(px(3.0))
        .h(px(4.0))
        .children(marks.iter().map(|mark| {
            let cell = div()
                .flex_1()
                .h(px(4.0))
                .rounded(px(2.0))
                .flex()
                .overflow_hidden();
            match mark {
                ProgressMark::Done => cell.bg(rgba(TONE_RUNNING)),
                ProgressMark::Current => cell
                    .child(div().w_1_2().h_full().bg(rgba(TONE_RUNNING)))
                    .child(
                        div()
                            .w_1_2()
                            .h_full()
                            .bg(rgba(with_alpha(TONE_RUNNING, 0x59))),
                    ),
                ProgressMark::Todo => cell.bg(rgba(0xcde8ff24)),
            }
        }))
}

pub(super) fn tone_color(
    tone: OverviewStatusTone,
    colors: super::super::overview_rows::OverviewTone,
) -> u32 {
    match tone {
        OverviewStatusTone::Running => colors.running,
        OverviewStatusTone::Idle => colors.idle,
        OverviewStatusTone::Failed => colors.failed,
        OverviewStatusTone::Approval => colors.approval,
        OverviewStatusTone::Review => colors.review,
        OverviewStatusTone::File => colors.file,
    }
}

/// 栏标题前的折叠箭头：展开朝下，折叠朝右。用图标而不是文字符号，避免和相邻文字粘在一起。
pub(super) fn group_chevron(collapsed: bool) -> gpui::AnyElement {
    let icon = gpui::svg()
        .path("icons/chevron-down.svg")
        .w(px(11.0))
        .h(px(11.0))
        .flex_none()
        .text_color(rgba(0x9db7cdff));
    if collapsed {
        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
            -std::f32::consts::FRAC_PI_2,
        )))
        .into_any_element()
    } else {
        icon.into_any_element()
    }
}
