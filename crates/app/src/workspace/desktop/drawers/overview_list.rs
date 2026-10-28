//! Left-drawer run overview. Rows are 48px for attention and projects;
//! agent rows grow when a segmented progress bar is present.

use super::super::overview_layout::{OverviewGroup, MIN_GROUP_HEIGHT};
use super::super::overview_rows::{
    overview_feedback, overview_tone, OverviewRow, OverviewSection, OVERVIEW_TEXT_COPY,
    OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use super::overview_parts::{action_label, group_chevron, row_copy, row_lead, tone_color};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::StyledExt;

impl MultisoulApp {
    /// 顶部三张统计卡，其后四栏固定顺序：需要你处理 → 运行中 → 待开始 → 想法。每栏有基准高度，
    /// 内容多了栏内滚动；折叠的栏只剩一行标题，空出的高度让给其余栏。
    pub(super) fn render_run_overview(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let screen = self.presented_overview();
        let mut counts = screen.summary();
        counts[0] += self.brain_overview().proposals.len();
        let cursor = self.desktop_workspace.overview_cursor;
        let sections = OverviewGroup::ORDER.map(|group| {
            let section = match group {
                OverviewGroup::Attention => screen.sections.first(),
                OverviewGroup::Running => screen.sections.get(1),
                OverviewGroup::Idle => screen.sections.get(2),
                OverviewGroup::Ideas => None,
            };
            (group, section)
        });
        let mut flat = 0usize;
        let mut groups = Vec::with_capacity(sections.len());
        for (group, section) in sections {
            // 幕僚活动的入口紧挨在想法栏之前；没有启用大脑时不显示。
            if group == OverviewGroup::Ideas && self.brain_enabled() {
                groups.push(self.render_brain_entry(cx));
            }
            let start = flat;
            flat += section.map_or(0, |section| section.rows.len());
            groups.push(self.render_overview_group(group, section, start, cursor, cx));
        }
        div()
            .id("desktop-agent-overview-list")
            .debug_selector(|| "desktop-agent-overview-list".to_string())
            .flex_1()
            .min_h(px(0.0))
            // Short drawers and floating panels can be smaller than the four groups' minima.
            // Keep the lower groups reachable when a question reserves space for its actions.
            .overflow_y_scroll()
            .px(px(4.0))
            .pb(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(self.render_overview_summary(counts, cx))
            .children(groups)
    }

    fn render_overview_group(
        &mut self,
        group: OverviewGroup,
        section: Option<&OverviewSection>,
        start: usize,
        cursor: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let feedback = overview_feedback(theme);
        let tone = overview_tone(theme);
        let collapsed = self.overview_collapse().is_collapsed(group);
        if collapsed && group == OverviewGroup::Ideas {
            // 折叠期间不渲染清单；展开时不要把期间新增的想法当成“刚新增”而滚动。
            self.desktop_workspace.ideas_seen_todo = None;
        }
        let heading = self.group_heading(group, section, collapsed, feedback, cx);
        // 大脑提议的下一步排在“需要你处理”最前面。
        let proposals = if group == OverviewGroup::Attention && !collapsed {
            let mut rows = self.render_question_rows(cx);
            rows.extend(self.render_proposal_rows(feedback, cx));
            rows
        } else {
            Vec::new()
        };
        let body = match (group, section) {
            (OverviewGroup::Ideas, _) => self.render_ideas_group_body(cx).into_any_element(),
            (_, Some(section)) => div()
                .id(ElementId::Name(
                    format!("desktop-run-body-{}", group.key()).into(),
                ))
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(section.rows.is_empty() && proposals.is_empty(), |body| {
                    body.child(group_empty(theme, group, true))
                })
                .children(proposals)
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
                .into_any_element(),
            (_, None) => div().into_any_element(),
        };
        let selector = format!("desktop-run-overview-{}", group.key());
        let mut shell = div()
            .id(ElementId::Name(
                format!("desktop-run-group-{}", group.key()).into(),
            ))
            .debug_selector(move || selector.clone())
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(group != OverviewGroup::Attention, |shell| {
                shell
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(rgba(theme.node_stroke))
            })
            .child(heading)
            .when(!collapsed, |shell| shell.child(body));
        let style = shell.style();
        if collapsed {
            style.flex_grow = Some(0.0);
            style.flex_shrink = Some(0.0);
        } else {
            style.flex_grow = Some(group.grow());
            // 展开的提议要完整可见（指令与理由），这时“需要你处理”不让出高度，由其余栏收缩。
            let question_heights = self.question_heights();
            let has_questions = group == OverviewGroup::Attention && !question_heights.is_empty();
            // Ideas includes fixed tabs and a composer; shrinking it to a single
            // status row would clip the composer even after the outer list scrolls.
            let keep = (group == OverviewGroup::Attention
                && (self.brain_proposal_open().is_some() || has_questions))
                || (group == OverviewGroup::Ideas && !question_heights.is_empty());
            style.flex_shrink = Some(if keep { 0.0 } else { 1.0 });
            let heights: Vec<f32> = match (group, section) {
                (OverviewGroup::Attention, Some(section)) => self
                    .proposal_heights()
                    .into_iter()
                    .chain(self.question_heights())
                    .chain(section.rows.iter().map(|row| row.height))
                    .collect(),
                (_, Some(section)) => section.rows.iter().map(|row| row.height).collect(),
                _ => Vec::new(),
            };
            let basis = if has_questions {
                // Keep the current question's nearby actions visible. Longer questionnaires
                // still scroll within this group instead of consuming every other section.
                (34.0 + heights.iter().map(|height| height + 2.0).sum::<f32>())
                    .clamp(MIN_GROUP_HEIGHT, 300.0)
            } else {
                group.basis_height(&heights)
            };
            style.flex_basis = Some(px(basis).into());
            style.min_size.height = Some(px(MIN_GROUP_HEIGHT).into());
        }
        shell
    }

    /// 栏标题：整行可点击折叠/展开（▾ / ▸），右侧是计数或说明。
    fn group_heading(
        &self,
        group: OverviewGroup,
        section: Option<&OverviewSection>,
        collapsed: bool,
        feedback: super::super::overview_rows::OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let hover = feedback.hover;
        let pressed = feedback.pressed;
        let title = match (group, section) {
            (OverviewGroup::Ideas, _) => "想法".to_owned(),
            (_, Some(section)) => section.title.clone(),
            _ => String::new(),
        };
        let todo = self
            .desktop_workspace
            .ideas
            .iter()
            .filter(|idea| idea.state == workspace::ideas::IdeaState::Todo)
            .count();
        let aside = match (group, section) {
            (OverviewGroup::Ideas, _) => {
                (todo > 0).then(|| count_badge(theme, todo, BadgeTone::Neutral))
            }
            // 琥珀色只给“需要你处理”（要你介入）；运行中用青色，待开始用灰蓝，与原型一致。
            (OverviewGroup::Attention, Some(section)) => {
                let extra = self.brain_overview().proposals.len();
                section
                    .count
                    .map(|count| count + extra)
                    .or((extra > 0).then_some(extra))
                    .map(|count| count_badge(theme, count, BadgeTone::Amber))
            }
            (_, Some(section)) => section
                .count
                .map(|count| count_badge(theme, count, BadgeTone::of(group))),
            _ => None,
        };
        div()
            .id(ElementId::Name(
                format!("desktop-run-toggle-{}", group.key()).into(),
            ))
            .debug_selector({
                let key = group.key();
                move || format!("desktop-run-toggle-{key}")
            })
            .flex_none()
            .h(px(30.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.update_overview_collapse(|collapse| collapse.toggle(group), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(OVERVIEW_TEXT_SECONDARY))
                    .font_semibold()
                    .text_color(rgba(theme.text_secondary))
                    .child(group_chevron(theme, collapsed))
                    .child(title),
            )
            .children(aside)
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
        let agent_row = row.is_agent;
        div()
            .id(ElementId::Name(id.clone().into()))
            .debug_selector(move || id.clone())
            .w_full()
            // 栏内滚动容器里行不得被压缩到小于内容，否则相邻行文字会重叠。
            .flex_none()
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
            .child(row_lead(theme, row, color, agent_row))
            .child(row_copy(row, color, theme))
            .children(decision_buttons(theme, row, index, cx))
            .child(action_label(theme, &row.action_label, agent_row))
    }
}

#[derive(Clone, Copy)]
enum BadgeTone {
    Amber,
    Running,
    Idle,
    Neutral,
}

impl BadgeTone {
    fn of(group: OverviewGroup) -> Self {
        match group {
            OverviewGroup::Attention => Self::Amber,
            OverviewGroup::Running => Self::Running,
            OverviewGroup::Idle => Self::Idle,
            OverviewGroup::Ideas => Self::Neutral,
        }
    }

    fn colors(self, theme: ui::Theme) -> (u32, u32) {
        let (fill, ink) = match self {
            Self::Amber => (
                ui::icons::with_alpha(theme.accent_warm, 0x26),
                theme.text_primary,
            ),
            Self::Running => (
                ui::icons::with_alpha(theme.accent_cyan, 0x24),
                theme.text_primary,
            ),
            Self::Idle => (
                ui::icons::with_alpha(theme.text_secondary, 0x24),
                theme.text_primary,
            ),
            Self::Neutral => (
                ui::icons::with_alpha(theme.accent, 0x2e),
                theme.text_primary,
            ),
        };
        if theme.is_light() {
            (fill, ink)
        } else {
            let base = crate::workspace::desktop::surface_theme::mix(theme.canvas_from, fill, 38);
            (ui::icons::with_alpha(base, 0xf5), ink)
        }
    }
}

/// 计数角标：按栏着色，琥珀＝要你介入，青色＝运行中，灰蓝＝待开始，中性蓝＝想法。
fn count_badge(theme: ui::Theme, count: usize, tone: BadgeTone) -> Div {
    let (background, foreground) = tone.colors(theme);
    div()
        .min_w(px(18.0))
        .h(px(18.0))
        .px(px(5.0))
        .rounded(px(9.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(background))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .font_semibold()
        .text_color(rgba(foreground))
        .child(count.to_string())
}

/// 栏内没有内容时的空态：标题加一行提示，在固定高度的栏内居中。
/// `fill` 为真时占满所在的栏（栏里只有它）；为假时按内容高度排在其他内容之上（想法栏里还有「已完成」时）。
pub(super) fn group_empty(theme: ui::Theme, group: OverviewGroup, fill: bool) -> Stateful<Div> {
    let (title, hint) = group.empty_copy();
    div()
        .id(ElementId::Name(
            format!("desktop-run-empty-{}", group.key()).into(),
        ))
        .w_full()
        .when(fill, |empty| empty.h_full().min_h(px(0.0)))
        .when(!fill, |empty| empty.py(px(14.0)))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .text_center()
        .child(
            div()
                .text_size(px(16.0))
                .text_color(rgba(theme.text_tertiary))
                .child(group.empty_icon()),
        )
        .child(
            div()
                .text_size(px(OVERVIEW_TEXT_PRIMARY))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .when(group != OverviewGroup::Idle, |empty| {
            empty.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_COPY))
                    .text_color(rgba(theme.text_tertiary))
                    .child(hint),
            )
        })
}

/// 概览里可直接处理的审批：（项目区域，卡片，run，请求标识）。没有请求标识或行不可用时没有。
pub(super) fn decision_target(row: &OverviewRow) -> Option<(String, u64, String, String)> {
    use super::super::overview_rows::OverviewAction;
    let OverviewAction::LocateApproval {
        area_id,
        node_id,
        run_id,
        request_id: Some(request_id),
        ..
    } = &row.action
    else {
        return None;
    };
    row.enabled.then(|| {
        (
            area_id.clone(),
            *node_id,
            run_id.clone(),
            request_id.clone(),
        )
    })
}

/// 能直接处理的审批行右侧的“允许 / 拒绝”。点击只处理审批，不触发整行的“定位”。
fn decision_buttons(
    theme: ui::Theme,
    row: &OverviewRow,
    index: usize,
    cx: &mut Context<MultisoulApp>,
) -> Option<Div> {
    use super::overview_brain::{pill, PillStyle};
    let (area_id, node_id, run_id, request_id) = decision_target(row)?;
    let make =
        |tag: &'static str, label: &'static str, allow: bool, cx: &mut Context<MultisoulApp>| {
            let (area_id, run_id, request_id) =
                (area_id.clone(), run_id.clone(), request_id.clone());
            pill(
                theme,
                ElementId::Name(format!("overview-{tag}-{index}").into()),
                label,
                if allow {
                    PillStyle::Primary
                } else {
                    PillStyle::Quiet
                },
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.decide_overview_approval(&area_id, node_id, &run_id, &request_id, allow, cx);
            }))
        };
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(make("allow", "允许", true, cx))
            .child(make("deny", "拒绝", false, cx)),
    )
}

#[cfg(test)]
#[path = "../../../../tests/unit/overview_decision_tests.rs"]
mod decision_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/overview_status_theme_tests.rs"]
mod theme_tests;
