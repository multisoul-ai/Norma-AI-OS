//! 概览行的组成部件：行首圆点、文案、状态角标、步骤条与操作文字。

use super::super::overview_rows::{
    OverviewRow, OverviewStatusTone, ProgressMark, OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY,
    OVERVIEW_TEXT_SECONDARY,
};
use gpui::{div, prelude::*, px, rgba, AnyView, App, Div, Window};
use ui::component::{tooltip::Tooltip, StyledExt};
use ui::icons::with_alpha;

pub(super) fn content_tooltip(content: String) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |window, cx| {
        Tooltip::element({
            let content = content.clone();
            move |_, _| {
                div()
                    .max_w(px(420.0))
                    .whitespace_normal()
                    .child(content.clone())
            }
        })
        .build(window, cx)
    }
}

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
            row.task_line.is_some() || !row.secondary.is_empty() || row.project_tag.is_some(),
            |copy| {
                let text = row
                    .task_line
                    .clone()
                    .unwrap_or_else(|| row.secondary.clone());
                copy.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(OVERVIEW_TEXT_COPY))
                        .text_color(rgba(theme.text_secondary))
                        .when_some(row.project_tag.clone(), |line, project| {
                            line.child(project_chip(theme, &project))
                        })
                        // 文字需要 flex_1：没有定宽的 flex 子项会被量成零宽，逐字换行并撑高整列。
                        .when(!text.is_empty(), |line| {
                            line.child(div().flex_1().min_w(px(0.0)).truncate().child(text))
                        }),
                )
            },
        )
        .when_some(row.progress.clone(), |copy, marks| {
            copy.child(progress_bar(theme, &marks))
        })
        .when_some(row.detail.clone(), |copy, detail| {
            copy.child(
                div()
                    .mt(px(if row.progress.is_some() { 1.0 } else { 0.0 }))
                    .text_size(px(OVERVIEW_TEXT_COPY))
                    .text_color(rgba(theme.text_secondary))
                    .child(detail),
            )
        })
}

pub(super) fn primary_line(row: &OverviewRow, _color: u32, theme: ui::Theme) -> Div {
    let agent_row = row.is_agent;
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .min_w(px(0.0))
        .child(
            div()
                .flex_shrink()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(OVERVIEW_TEXT_PRIMARY))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(theme.text_primary))
                .child(row.primary.clone()),
        )
        .when_some(
            row.status_label.clone().filter(|_| agent_row),
            |line, label| {
                let (background, foreground) = status_colors(theme, row.status_tone);
                line.child(status_chip(&label, background, foreground))
            },
        )
        .when_some(row.approval_badge.clone(), |line, badge| {
            let (background, foreground) = status_colors(theme, OverviewStatusTone::Approval);
            line.child(status_chip(&badge, background, foreground))
        })
}

pub(super) fn status_colors(theme: ui::Theme, tone: OverviewStatusTone) -> (u32, u32) {
    let ink = match tone {
        OverviewStatusTone::Running => theme.accent_cyan,
        OverviewStatusTone::Approval => theme.accent_warm,
        _ => theme.text_secondary,
    };
    // Keep small status text readable even when a bright wallpaper shows through the 60% drawer.
    let fill = super::super::surface_theme::mix(theme.canvas_from, ink, 28);
    (with_alpha(fill, 0xf5), ink)
}

/// 智能体行里标明所属项目的小标签：浅蓝底、不抢状态色。
fn project_chip(theme: ui::Theme, label: &str) -> Div {
    div()
        .flex_shrink()
        .min_w(px(0.0))
        .truncate()
        .px(px(6.0))
        .rounded(px(5.0))
        .bg(rgba(ui::icons::with_alpha(theme.accent, 0x1f)))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(theme.text_secondary))
        .child(label.to_owned())
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
pub(super) fn row_lead(theme: ui::Theme, row: &OverviewRow, color: u32, agent_row: bool) -> Div {
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
                .border_color(rgba(ui::icons::with_alpha(theme.text_primary, 0x33)))
                .flex()
                .items_center()
                .justify_center()
                .font_family("Menlo")
                .text_size(px(OVERVIEW_TEXT_SECONDARY))
                .font_semibold()
                .text_color(rgba(theme.text_tertiary))
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

pub(super) fn action_label(theme: ui::Theme, label: &str, agent_row: bool) -> Div {
    div()
        .flex_none()
        .px(px(4.0))
        .when(agent_row, |action| action.mt(px(2.0)))
        .text_size(px(OVERVIEW_TEXT_COPY))
        .text_color(rgba(theme.text_secondary))
        .child(format!("{label} ↗"))
}

/// 步骤条：已完成实心，当前一格左半实心右半淡色，其余为底色。
pub(super) fn progress_bar(theme: ui::Theme, marks: &[ProgressMark]) -> Div {
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
                ProgressMark::Done => cell.bg(rgba(theme.accent_cyan)),
                ProgressMark::Current => cell
                    .child(div().w_1_2().h_full().bg(rgba(theme.accent_cyan)))
                    .child(
                        div()
                            .w_1_2()
                            .h_full()
                            .bg(rgba(with_alpha(theme.accent_cyan, 0x59))),
                    ),
                ProgressMark::Todo => {
                    cell.bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x24)))
                }
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

/// 栏标题前的折叠箭头：与原型一致的 8×5 实心小三角，展开朝下，折叠朝右。
pub(super) fn group_chevron(theme: ui::Theme, collapsed: bool) -> gpui::AnyElement {
    let icon = gpui::svg()
        .path("icons/triangle-down.svg")
        .w(px(8.0))
        .h(px(5.0))
        .flex_none()
        .text_color(rgba(theme.text_secondary));
    let icon = if collapsed {
        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
            -std::f32::consts::FRAC_PI_2,
        )))
    } else {
        icon
    };
    div()
        .w(px(8.0))
        .h(px(8.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(icon)
        .into_any_element()
}
