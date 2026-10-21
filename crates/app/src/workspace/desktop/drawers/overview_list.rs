//! Left-drawer run overview. Rows are 48px for attention and projects;
//! agent rows grow when a segmented progress bar is present.

use super::super::overview_layout::{OverviewGroup, MIN_GROUP_HEIGHT};
use super::super::overview_rows::{
    overview_feedback, overview_tone, OverviewAction, OverviewRow, OverviewSection,
    OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use super::overview_parts::{action_label, group_chevron, row_copy, row_lead, tone_color};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::StyledExt;

const AMBER_TINT: u32 = 0xf2c77326;
const AMBER_TEXT: u32 = 0xf6dfaaff;
const GROUP_TITLE: u32 = 0xa9c3daff;
const GROUP_META: u32 = 0x8ea8bfff;
const COPY_STEP: u32 = 0xcfe3f4ff;
const GROUP_RULE: u32 = 0xcde8ff14;

impl MultisoulApp {
    /// 四栏固定顺序：需要你处理 → 当前项目 → 想法 → 其他项目。每栏有基准高度，
    /// 内容多了栏内滚动；折叠的栏只剩一行标题，空出的高度让给其余栏。
    pub(super) fn render_run_overview(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let screen = self.presented_overview();
        let cursor = self.desktop_workspace.overview_cursor;
        let sections = OverviewGroup::ORDER.map(|group| {
            let section = match group {
                OverviewGroup::Attention => screen.sections.first(),
                OverviewGroup::Current => screen.sections.get(1),
                OverviewGroup::Ideas => None,
                OverviewGroup::Other => screen.sections.get(2),
            };
            (group, section)
        });
        let mut flat = 0usize;
        let mut groups = Vec::with_capacity(sections.len());
        for (group, section) in sections {
            let start = flat;
            flat += section.map_or(0, |section| section.rows.len());
            groups.push(self.render_overview_group(group, section, start, cursor, cx));
        }
        div()
            .id("desktop-agent-overview-list")
            .debug_selector(|| "desktop-agent-overview-list".to_string())
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .px(px(4.0))
            .pb(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
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
        let heading = self.group_heading(group, section, collapsed, feedback, cx);
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
                .when(section.rows.is_empty(), |body| {
                    body.child(group_empty(group))
                })
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
                    .border_color(rgba(GROUP_RULE))
            })
            .child(heading)
            .when(!collapsed, |shell| shell.child(body));
        let style = shell.style();
        if collapsed {
            style.flex_grow = Some(0.0);
            style.flex_shrink = Some(0.0);
        } else {
            style.flex_grow = Some(group.grow());
            style.flex_shrink = Some(1.0);
            style.flex_basis = Some(px(group.base_height()).into());
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
            (OverviewGroup::Ideas, _) => (todo > 0).then(|| count_badge(todo, false)),
            (_, Some(section)) => match section.count {
                Some(count) => Some(count_badge(count, true)),
                None => section.meta.clone().map(|meta| {
                    div()
                        .text_size(px(OVERVIEW_TEXT_SECONDARY))
                        .text_color(rgba(GROUP_META))
                        .child(meta)
                }),
            },
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
                    .text_color(rgba(GROUP_TITLE))
                    .child(group_chevron(collapsed))
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

/// 计数角标：`amber` 为需要你处理（琥珀），否则为普通计数（中性蓝灰）。
fn count_badge(count: usize, amber: bool) -> Div {
    let (background, foreground) = if amber {
        (AMBER_TINT, AMBER_TEXT)
    } else {
        (0x8fcaff2e, 0xd4eaffff)
    };
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
pub(super) fn group_empty(group: OverviewGroup) -> Stateful<Div> {
    let (title, hint) = group.empty_copy();
    div()
        .id(ElementId::Name(
            format!("desktop-run-empty-{}", group.key()).into(),
        ))
        .size_full()
        .min_h(px(0.0))
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
                .text_color(rgba(0x7ea4c4ff))
                .child(group.empty_icon()),
        )
        .child(
            div()
                .text_size(px(OVERVIEW_TEXT_PRIMARY))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(COPY_STEP))
                .child(title),
        )
        .when(group != OverviewGroup::Other, |empty| {
            empty.child(
                div()
                    .text_size(px(OVERVIEW_TEXT_COPY))
                    .text_color(rgba(GROUP_META))
                    .child(hint),
            )
        })
}
