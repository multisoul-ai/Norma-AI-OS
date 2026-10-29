//! 想法栏顶部的分段按钮（待办 / 已指派 / 已完成）、当前分段的「清空」行与空状态。
//!
//! 设计注记（Material 3 segmented button）：
//! - 三段共用一圈轮廓，选中段填充 tonal 色并显示数量；hover / 按下各加一档状态层，键盘用 ←/→ 之外的现有浏览方式。
//! - 首尾两段自己绘制外侧圆角（GPUI 的 overflow_hidden 只做矩形裁剪）。
//! - 切换即时生效、不加动画；切换时清掉键盘光标与展开的菜单，避免操作到看不见的行。

use super::super::ideas_rows::IdeasTab;
use super::super::overview_rows::OverviewFeedback;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId};

const HEIGHT: f32 = 32.0;

impl MultisoulApp {
    pub(super) fn set_ideas_tab(&mut self, tab: IdeasTab, cx: &mut Context<Self>) {
        if self.desktop_workspace.ideas_tab == tab {
            return;
        }
        self.desktop_workspace.ideas_tab = tab;
        self.desktop_workspace.ideas_cursor = None;
        self.desktop_workspace.ideas_menu = None;
        cx.notify();
    }

    /// 分段按钮：`counts` 按 `IdeasTab::ALL` 的顺序给出每段条数。
    pub(super) fn render_ideas_tabs(
        &self,
        counts: [usize; 3],
        feedback: OverviewFeedback,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.desktop_theme();
        let current = self.desktop_workspace.ideas_tab;
        div()
            .flex_none()
            .px(px(12.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .child(
                div()
                    .debug_selector(|| "desktop-ideas-tabs".to_string())
                    .h(px(HEIGHT))
                    .flex()
                    .rounded(px(HEIGHT / 2.0))
                    .border_1()
                    .border_color(rgba(theme.node_stroke))
                    .children(IdeasTab::ALL.into_iter().zip(counts).enumerate().map(
                        |(index, (tab, count))| {
                            let selected = tab == current;
                            let states = tab_feedback(theme, selected, feedback);
                            let radius = px(HEIGHT / 2.0);
                            div()
                                .id(ElementId::Name(
                                    format!("desktop-ideas-tab-{}", tab.key()).into(),
                                ))
                                .debug_selector(move || format!("desktop-ideas-tab-{}", tab.key()))
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(px(5.0))
                                .when(index == 0, |segment| segment.rounded_l(radius))
                                .when(index == 2, |segment| segment.rounded_r(radius))
                                .when(index > 0, |segment| {
                                    segment.border_l_1().border_color(rgba(theme.node_stroke))
                                })
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .cursor_pointer()
                                .when(selected, |segment| {
                                    segment
                                        .bg(rgba(states.rest))
                                        .text_color(rgba(theme.text_primary))
                                })
                                .when(!selected, |segment| {
                                    segment.text_color(rgba(theme.text_secondary))
                                })
                                .hover(move |style| {
                                    style
                                        .bg(rgba(states.hover))
                                        .text_color(rgba(theme.text_primary))
                                })
                                .active(move |style| style.bg(rgba(states.pressed)))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.set_ideas_tab(tab, cx);
                                }))
                                .child(tab.label())
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .opacity(0.8)
                                        .child(count.to_string()),
                                )
                        },
                    )),
            )
    }
}

pub(super) fn tab_feedback(
    theme: ui::Theme,
    selected: bool,
    feedback: OverviewFeedback,
) -> OverviewFeedback {
    if selected {
        let fill = |amount| {
            ui::icons::with_alpha(
                super::super::surface_theme::mix(theme.canvas_from, theme.accent, amount),
                0xf5,
            )
        };
        OverviewFeedback {
            rest: fill(50),
            hover: fill(70),
            pressed: fill(90),
            focus: feedback.focus,
        }
    } else {
        feedback
    }
}

/// 当前分段顶部的一行：左侧说明，右侧由带点击回调的「清空」文字按钮（已指派、已完成才有）。
pub(super) fn clear_row(
    theme: ui::Theme,
    id: &'static str,
    hint: &'static str,
    feedback: OverviewFeedback,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Div {
    let hover = feedback.hover;
    let pressed = feedback.pressed;
    div()
        .h(px(28.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(11.0))
        .text_color(rgba(theme.text_tertiary))
        .child(hint)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .h(px(24.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(12.0))
                .cursor_pointer()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .hover(move |style| style.bg(rgba(hover)).text_color(rgba(theme.text_primary)))
                .active(move |style| style.bg(rgba(pressed)))
                .on_click(on_click)
                .child(ui::i18n::text("drawers.clear")),
        )
}

/// 分段内没有内容时的居中提示。
pub(super) fn tab_empty(theme: ui::Theme, title: &'static str, hint: &'static str) -> Div {
    // flex_none + 较小的竖向留白：栏高不够时由列表滚动，而不是把提示压进输入框下面被截断。
    div()
        .w_full()
        .flex_none()
        .py(px(16.0))
        .px(px(24.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.0))
        .text_center()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(hint),
        )
}
