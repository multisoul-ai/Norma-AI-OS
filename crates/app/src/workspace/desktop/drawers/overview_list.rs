//! Left-drawer run overview. Rows are 48px for attention and projects;
//! agent rows grow when a segmented progress bar is present.

use super::super::overview_rows::{
    overview_feedback, overview_tone, OverviewAction, OverviewRow, OverviewSection,
    OverviewStatusTone, ProgressMark, OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY,
    OVERVIEW_TEXT_SECONDARY, TONE_RUNNING,
};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::StyledExt;
use ui::icons::with_alpha;

const AMBER_TINT: u32 = 0xf2c77326;
const AMBER_TEXT: u32 = 0xf6dfaaff;
const GROUP_TITLE: u32 = 0xa9c3daff;
const GROUP_META: u32 = 0x8ea8bfff;
const COPY_PRIMARY: u32 = 0xeef6ffff;
const COPY_SECONDARY: u32 = 0xa9c0d5ff;
const COPY_STEP: u32 = 0xcfe3f4ff;
const TRAIL: u32 = 0x9fc7eaff;
const GROUP_RULE: u32 = 0xcde8ff14;

impl MultisoulApp {
    pub(super) fn render_run_overview(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let screen = self.presented_overview();
        let cursor = self.desktop_workspace.overview_cursor;
        let feedback = overview_feedback(theme);
        let tone = overview_tone(theme);
        let mut flat = 0usize;
        div()
            .id("desktop-agent-overview-list")
            .debug_selector(|| "desktop-agent-overview-list".to_string())
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px(px(4.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .children(screen.sections.iter().map(|section| {
                let start = flat;
                flat += section.rows.len();
                self.render_overview_section(section, start, cursor, feedback, tone, theme, cx)
            }))
    }

    fn render_overview_section(
        &self,
        section: &OverviewSection,
        start: usize,
        cursor: Option<usize>,
        feedback: super::super::overview_rows::OverviewFeedback,
        tone: super::super::overview_rows::OverviewTone,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let selector = section_selector(&section.title).to_string();
        div()
            .mt(px(4.0))
            .when(start > 0, |group| {
                group
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(rgba(GROUP_RULE))
            })
            .debug_selector(move || selector.clone())
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(section_heading(section, theme))
            .children(section.rows.iter().enumerate().map(|(offset, row)| {
                self.render_overview_row(
                    row,
                    start + offset,
                    cursor == Some(start + offset),
                    feedback,
                    tone,
                    theme,
                    cx,
                )
            }))
    }

    fn render_overview_row(
        &self,
        row: &OverviewRow,
        index: usize,
        focused: bool,
        feedback: super::super::overview_rows::OverviewFeedback,
        tone: super::super::overview_rows::OverviewTone,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let color = tone_color(row.status_tone, tone);
        let action = row.action.clone();
        let prelight = row.prelight.clone();
        let enabled = row.enabled;
        let id = format!("desktop-overview-row-{index}");
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let height = row.height;
        let agent_row = matches!(
            row.action,
            OverviewAction::LocateAgent { .. } | OverviewAction::StopEgo { .. }
        );
        div()
            .id(ElementId::Name(id.clone().into()))
            .debug_selector(move || id.clone())
            .w_full()
            .when(height <= 48.0, |row| row.h(px(height)))
            .min_h(px(height))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .when(!agent_row, |row| row.items_center())
            .when(agent_row, |row| row.items_start())
            .gap(px(10.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(if focused { feedback.focus } else { 0x0000_0000 }))
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(rgba(hover)))
                    .active(move |style| style.bg(rgba(pressed)))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.activate_overview_action(action.clone(), window, cx);
                    }))
            })
            .when(!enabled, |row| row.cursor_default().opacity(0.55))
            .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                if !enabled {
                    return;
                }
                if *hovered {
                    app.prelight_overview(prelight.clone(), cx);
                } else if app.desktop_workspace.overview_prelight == prelight {
                    let fallback = app.desktop_workspace.overview_cursor.and_then(|index| {
                        app.presented_overview()
                            .row(index)
                            .and_then(|row| row.prelight.clone())
                    });
                    app.prelight_overview(fallback, cx);
                }
            }))
            .child(row_lead(row, color, agent_row))
            .child(row_copy(row, color, theme))
            .child(action_label(&row.action_label, agent_row))
    }
}

fn section_heading(section: &OverviewSection, _theme: ui::Theme) -> Div {
    div()
        .h(px(30.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(OVERVIEW_TEXT_SECONDARY))
                .font_semibold()
                .text_color(rgba(GROUP_TITLE))
                .child(section.title.clone()),
        )
        .when_some(section.count, |heading, count| {
            heading.child(count_badge(count))
        })
        .when(section.count.is_none(), |heading| {
            heading.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .text_color(rgba(GROUP_META))
                    .child(section.meta.clone().unwrap_or_default()),
            )
        })
}

fn count_badge(count: usize) -> Div {
    div()
        .min_w(px(18.0))
        .h(px(18.0))
        .px(px(5.0))
        .rounded(px(9.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(AMBER_TINT))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .font_semibold()
        .text_color(rgba(AMBER_TEXT))
        .child(count.to_string())
}

fn row_copy(row: &OverviewRow, color: u32, theme: ui::Theme) -> Div {
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

fn primary_line(row: &OverviewRow, color: u32, _theme: ui::Theme) -> Div {
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

fn status_chip(label: &str, background: u32, foreground: u32) -> Div {
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
fn row_lead(row: &OverviewRow, color: u32, agent_row: bool) -> Div {
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

fn action_label(label: &str, agent_row: bool) -> Div {
    div()
        .flex_none()
        .px(px(4.0))
        .when(agent_row, |action| action.mt(px(2.0)))
        .text_size(px(OVERVIEW_TEXT_COPY))
        .text_color(rgba(TRAIL))
        .child(format!("{label} ↗"))
}

/// 步骤条：已完成实心，当前一格左半实心右半淡色，其余为底色。
fn progress_bar(marks: &[ProgressMark]) -> Div {
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

fn section_selector(title: &str) -> &'static str {
    if title.starts_with("当前项目") {
        "desktop-run-overview-current"
    } else if title == "其他项目" {
        "desktop-run-overview-other"
    } else {
        "desktop-run-overview-attention"
    }
}

fn tone_color(tone: OverviewStatusTone, colors: super::super::overview_rows::OverviewTone) -> u32 {
    match tone {
        OverviewStatusTone::Running => colors.running,
        OverviewStatusTone::Idle => colors.idle,
        OverviewStatusTone::Failed => colors.failed,
        OverviewStatusTone::Approval => colors.approval,
        OverviewStatusTone::Review => colors.review,
        OverviewStatusTone::File => colors.file,
    }
}
