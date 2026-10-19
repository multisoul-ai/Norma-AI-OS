//! Left-drawer run overview. Rows are 48px for attention and projects;
//! agent rows grow when a segmented progress bar is present.

use super::super::overview_rows::{
    overview_feedback, overview_tone, OverviewRow, OverviewSection, OverviewStatusTone,
    ProgressMark, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::StyledExt;
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn render_run_overview(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let screen = self.presented_overview();
        let cursor = self.desktop_workspace.overview_cursor;
        let feedback = overview_feedback(theme);
        let tone = overview_tone(theme);
        let workspace = self
            .desktop_workspace
            .activity_summary
            .as_ref()
            .map(|summary| summary.name.clone())
            .unwrap_or_default();
        let mut flat = 0usize;
        div()
            .id("desktop-agent-overview-list")
            .debug_selector(|| "desktop-agent-overview-list".to_string())
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px(px(12.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(overview_heading(&workspace, theme))
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
            .mt(px(8.0))
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
        div()
            .id(ElementId::Name(id.clone().into()))
            .debug_selector(move || id.clone())
            .w_full()
            .when(height <= 48.0, |row| row.h(px(height)))
            .min_h(px(height))
            .px(px(8.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(8.0))
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
            .child(status_dot(color))
            .child(row_copy(row, color, theme))
            .child(action_label(&row.action_label, theme))
    }
}

fn overview_heading(workspace: &str, theme: ui::Theme) -> Div {
    div()
        .pt(px(12.0))
        .pb(px(2.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_semibold()
                .text_color(rgba(theme.text_primary))
                .child("运行概览"),
        )
        .when(!workspace.is_empty(), |heading| {
            heading.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .text_color(rgba(theme.text_secondary))
                    .child(workspace.to_owned()),
            )
        })
}

fn section_heading(section: &OverviewSection, theme: ui::Theme) -> Div {
    div()
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(OVERVIEW_TEXT_SECONDARY))
                .font_semibold()
                .text_color(rgba(theme.text_secondary))
                .child(section.title.clone()),
        )
        .when_some(section.count, |heading, count| {
            heading.child(count_badge(count, theme))
        })
        .when(section.count.is_none(), |heading| {
            heading.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .text_color(rgba(theme.text_tertiary))
                    .child(section.meta.clone().unwrap_or_default()),
            )
        })
}

fn count_badge(count: usize, theme: ui::Theme) -> Div {
    div()
        .px(px(6.0))
        .h(px(18.0))
        .rounded_full()
        .flex()
        .items_center()
        .bg(rgba(with_alpha(theme.accent_warm, 0x33)))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(theme.text_primary))
        .child(count.to_string())
}

fn row_copy(row: &OverviewRow, color: u32, theme: ui::Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(1.0))
        .child(primary_line(row, color, theme))
        .when(
            row.task_line.is_some() || !row.secondary.is_empty(),
            |copy| {
                copy.child(
                    div()
                        .text_size(px(OVERVIEW_TEXT_SECONDARY))
                        .text_color(rgba(theme.text_secondary))
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
            copy.child(progress_bar(&marks, theme))
        })
        .when_some(row.detail.clone(), |copy, detail| {
            copy.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .text_color(rgba(theme.text_secondary))
                    .child(detail),
            )
        })
}

fn primary_line(row: &OverviewRow, color: u32, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_w(px(0.0))
        .child(
            div()
                .flex_shrink()
                .text_size(px(OVERVIEW_TEXT_PRIMARY))
                .text_color(rgba(theme.text_primary))
                .child(row.primary.clone()),
        )
        .when_some(
            row.status_label.clone().filter(|_| row.task_line.is_some()),
            |line, label| line.child(status_chip(&label, color)),
        )
        .when_some(row.approval_badge.clone(), |line, badge| {
            line.child(status_chip(&badge, theme.accent_warm))
        })
}

fn status_chip(label: &str, color: u32) -> Div {
    div()
        .px(px(6.0))
        .h(px(18.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .flex_shrink()
        .bg(rgba(with_alpha(color, 0x28)))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(color))
        .child(label.to_owned())
}

fn status_dot(color: u32) -> Div {
    div()
        .w(px(6.0))
        .h(px(6.0))
        .rounded_full()
        .flex_shrink()
        .bg(rgba(color))
}

fn action_label(label: &str, theme: ui::Theme) -> Div {
    div()
        .flex_shrink()
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(theme.accent_blue))
        .child(format!("{label} ›"))
}

fn progress_bar(marks: &[ProgressMark], theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(3.0))
        .h(px(4.0))
        .children(marks.iter().map(|mark| {
            let color = match mark {
                ProgressMark::Done => theme.accent_cyan,
                ProgressMark::Current => theme.accent,
                ProgressMark::Todo => with_alpha(theme.text_tertiary, 0x66),
            };
            div().flex_1().h(px(4.0)).rounded(px(2.0)).bg(rgba(color))
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
