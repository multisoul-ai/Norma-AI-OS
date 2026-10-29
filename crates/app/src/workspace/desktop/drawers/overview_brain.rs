//! 运行概览里和后台大脑有关的部分：审批行的“允许 / 拒绝”，以及“幕僚活动”入口行。

use super::super::overview_rows::{
    OVERVIEW_TEXT_COPY, OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};

impl MultisoulApp {
    /// 概览审批行上的“允许 / 拒绝”：不经过大脑，直接在拥有该卡片的项目里处理。
    pub(super) fn decide_overview_approval(
        &mut self,
        area_id: &str,
        node_id: u64,
        run_id: &str,
        request_id: &str,
        allow: bool,
        cx: &mut Context<Self>,
    ) {
        let applied = self.with_overview_session(area_id, cx, |app, _| {
            app.decide_approval_by_user(node_id, run_id, request_id, allow)
        });
        if applied != Some(true) {
            self.desktop_workspace.error =
                Some(ui::i18n::LocalizedText::new("chrome.brain_approval_closed"));
        }
        cx.notify();
    }

    pub(crate) fn with_overview_session<R>(
        &mut self,
        area_id: &str,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut MultisoulApp, &mut Context<MultisoulApp>) -> R,
    ) -> Option<R> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id.to_owned()))?
            .clone();
        Some(session.update(cx, run))
    }

    /// 左抽屉里的“幕僚活动”入口：点开面板；数字是上次打开之后的新自主动作。
    pub(super) fn render_brain_entry(&mut self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let theme = self.desktop_theme();
        let unseen = self.brain_unseen_actions();
        div()
            .id("desktop-brain-entry")
            .debug_selector(|| "desktop-brain-entry".to_string())
            .flex_none()
            .h(px(46.0))
            .mx(px(4.0))
            .mt(px(6.0))
            .mb(px(2.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(9.0))
            .bg(rgba(ui::icons::with_alpha(theme.accent_violet, 0x26)))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(ui::icons::with_alpha(theme.accent_violet, 0x3d))))
            .active(|style| style.bg(rgba(ui::icons::with_alpha(theme.accent_violet, 0x55))))
            .on_click(cx.listener(|app, _, window, cx| {
                cx.stop_propagation();
                app.toggle_brain_trace(window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(OVERVIEW_TEXT_PRIMARY))
                            .text_color(rgba(theme.text_primary))
                            .child(ui::i18n::text("chrome.brain_entry_title")),
                    )
                    .child(
                        div()
                            .text_size(px(OVERVIEW_TEXT_COPY))
                            .text_color(rgba(theme.text_secondary))
                            .child(ui::i18n::text("chrome.brain_entry_hint")),
                    ),
            )
            .when(unseen > 0, |row| {
                row.child(
                    div()
                        .flex_none()
                        .px(px(7.0))
                        .rounded(px(9.0))
                        .bg(rgba(theme.accent))
                        .text_size(px(OVERVIEW_TEXT_SECONDARY))
                        .text_color(rgba(crate::workspace::desktop::surface_theme::on_accent(
                            theme,
                        )))
                        .child(unseen.to_string()),
                )
            })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PillStyle {
    Primary,
    Quiet,
    Disabled,
}

pub(super) fn pill_colors(theme: ui::Theme, style: PillStyle) -> (u32, u32, u32, u32) {
    use super::super::surface_theme::{accent_fill, mix, on_accent};
    match style {
        PillStyle::Primary => (
            theme.accent,
            accent_fill(theme, 26),
            accent_fill(theme, 48),
            on_accent(theme),
        ),
        PillStyle::Quiet => (
            ui::icons::with_alpha(mix(theme.canvas_from, theme.text_primary, 18), 0xf5),
            ui::icons::with_alpha(mix(theme.canvas_from, theme.text_primary, 32), 0xf5),
            ui::icons::with_alpha(mix(theme.canvas_from, theme.text_primary, 46), 0xf5),
            theme.text_primary,
        ),
        PillStyle::Disabled => (
            ui::icons::with_alpha(theme.text_primary, 0x0a),
            ui::icons::with_alpha(theme.text_primary, 0x0a),
            ui::icons::with_alpha(theme.text_primary, 0x0a),
            theme.text_tertiary,
        ),
    }
}

/// 概览里的小胶囊按钮：整块可点，悬停与按下各有一档颜色；与想法清单的胶囊同一做法。
pub(super) fn pill(
    theme: ui::Theme,
    id: impl Into<ElementId>,
    label: &'static str,
    style: PillStyle,
) -> Stateful<Div> {
    let (fill, hover, pressed, text) = pill_colors(theme, style);
    div()
        .id(id)
        .h(px(22.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .rounded(px(6.0))
        .bg(rgba(fill))
        .text_size(px(OVERVIEW_TEXT_SECONDARY))
        .text_color(rgba(text))
        .when(style != PillStyle::Disabled, |pill| {
            pill.cursor_pointer()
                .hover(move |s| s.bg(rgba(hover)))
                .active(move |s| s.bg(rgba(pressed)))
        })
        .child(label)
}
