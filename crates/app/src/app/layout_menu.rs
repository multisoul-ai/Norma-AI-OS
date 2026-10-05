//! 底部工具条的布局策略菜单：只改变后续卡片的落位策略，重排由显式按钮确认。

use super::chrome::chrome_shadow;
use super::MultisoulApp;
use crate::effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX};
use canvas::{CanvasRect, GridCardSizing, GridFill};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, MouseButton, Stateful, Window};
use ui::icons::with_alpha;
use ui::Theme;

/// 布局菜单面板宽度（与 render_layout_menu 的 w(px(294.0)) 一致）。
pub(crate) const LAYOUT_MENU_WIDTH_PX: f32 = 294.0;
/// Dock 底部 22 + 高度 52 + 浮层间距 8 = 82px。
pub(crate) const LAYOUT_MENU_BOTTOM_PX: f32 = COMMAND_DOCK_BOTTOM_PX + COMMAND_DOCK_HEIGHT_PX + 8.0;
/// 布局菜单面板高度（内容推导：heading 20 + 填充块 49 + 轨道 26 + 尺寸块 49 + reflow 32 + gap×4=40 + padding×2=24）。
pub(crate) const LAYOUT_MENU_HEIGHT_PX: f32 = 240.0;

/// 当菜单打开时，返回菜单在屏幕空间的矩形（用于原生浏览器视图遮挡判断）。
/// 关闭时返回 None，避免菜单未打开时误隐藏浏览器。
pub(crate) fn layout_menu_screen_rect(open: bool, win_w: f32, win_h: f32) -> Option<CanvasRect> {
    if !open {
        return None;
    }
    let left = (win_w - LAYOUT_MENU_WIDTH_PX) / 2.0;
    let top = win_h - LAYOUT_MENU_BOTTOM_PX - LAYOUT_MENU_HEIGHT_PX;
    Some(CanvasRect::new(
        left,
        top,
        LAYOUT_MENU_WIDTH_PX,
        LAYOUT_MENU_HEIGHT_PX,
    ))
}

/// 判断浏览器卡片的内容区矩形是否被某个 GPUI 浮层遮挡。
/// 与浮层矩形相交（含边界）即判定为遮挡。
pub(crate) fn browser_content_occluded_by_overlay(
    content_rect: CanvasRect,
    overlay_rect: Option<CanvasRect>,
) -> bool {
    overlay_rect.is_some_and(|overlay| content_rect.intersects(overlay))
}

impl MultisoulApp {
    pub(crate) fn on_toolbar_toggle_layout_menu(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.toggle_layout_menu();
        cx.notify();
    }

    pub(crate) fn on_layout_fill_row(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.fill = GridFill::RowFirst;
        cx.notify();
    }

    pub(crate) fn on_layout_fill_column(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.fill = GridFill::ColumnFirst;
        cx.notify();
    }

    pub(crate) fn on_layout_decrease_tracks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.decrease_track_count();
        cx.notify();
    }

    pub(crate) fn on_layout_increase_tracks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.increase_track_count();
        cx.notify();
    }

    pub(crate) fn on_layout_fixed_size(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.sizing = GridCardSizing::Fixed;
        cx.notify();
    }

    pub(crate) fn on_layout_fit_viewport(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout_strategy.sizing = GridCardSizing::FitViewport;
        cx.notify();
    }

    pub(crate) fn on_layout_reflow(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reflow_tiled_nodes(window);
        self.interaction.close_layout_menu();
        self.mark_canvas_dirty();
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn render_layout_menu(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let strategy = self.layout_strategy;
        let track_label = strategy.track_label();

        div()
            .absolute()
            .bottom(px(LAYOUT_MENU_BOTTOM_PX))
            .left(px(0.0))
            .right(px(0.0))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("layout-menu")
                    .debug_selector(|| "layout-menu".to_string())
                    .w(px(294.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .p(px(12.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x42)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xf2)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(menu_heading(theme, ui::i18n::text("chrome.layout")))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(menu_label(theme, ui::i18n::text("chrome.fill_direction")))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(segmented_button(
                                        theme,
                                        ui::i18n::text("chrome.rows_down"),
                                        "layout-menu-fill-row",
                                        strategy.fill == GridFill::RowFirst,
                                        cx.listener(Self::on_layout_fill_row),
                                    ))
                                    .child(segmented_button(
                                        theme,
                                        ui::i18n::text("chrome.columns_right"),
                                        "layout-menu-fill-column",
                                        strategy.fill == GridFill::ColumnFirst,
                                        cx.listener(Self::on_layout_fill_column),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(menu_label(theme, track_label))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(stepper_button(
                                        theme,
                                        "−",
                                        "layout-menu-track-decrease",
                                        cx.listener(Self::on_layout_decrease_tracks),
                                    ))
                                    .child(
                                        div()
                                            .id("layout-menu-track-count")
                                            .debug_selector(|| {
                                                "layout-menu-track-count".to_string()
                                            })
                                            .w(px(32.0))
                                            .text_center()
                                            .text_color(rgba(theme.text_primary))
                                            .text_size(px(13.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(strategy.track_count.to_string()),
                                    )
                                    .child(stepper_button(
                                        theme,
                                        "+",
                                        "layout-menu-track-increase",
                                        cx.listener(Self::on_layout_increase_tracks),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(menu_label(theme, ui::i18n::text("chrome.card_size")))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(segmented_button(
                                        theme,
                                        ui::i18n::text("chrome.fixed_size"),
                                        "layout-menu-size-fixed",
                                        strategy.sizing == GridCardSizing::Fixed,
                                        cx.listener(Self::on_layout_fixed_size),
                                    ))
                                    .child(segmented_button(
                                        theme,
                                        ui::i18n::text("chrome.fit_visible_area"),
                                        "layout-menu-size-fit",
                                        strategy.sizing == GridCardSizing::FitViewport,
                                        cx.listener(Self::on_layout_fit_viewport),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("layout-menu-reflow")
                            .debug_selector(|| "layout-menu-reflow".to_string())
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.0))
                            .cursor_pointer()
                            .bg(rgba(with_alpha(theme.accent_blue, 0x70)))
                            .text_color(rgba(theme.text_primary))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(ui::i18n::text("chrome.reflow_the_current_cards"))
                            .hover(move |style| style.bg(rgba(with_alpha(theme.accent_blue, 0x98))))
                            .on_click(cx.listener(Self::on_layout_reflow)),
                    ),
            )
    }
}

fn menu_heading(theme: Theme, label: &'static str) -> Div {
    div()
        .text_color(rgba(theme.text_primary))
        .text_size(px(14.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label)
}

fn menu_label(theme: Theme, label: &'static str) -> Div {
    div()
        .text_color(rgba(theme.text_tertiary))
        .text_size(px(11.0))
        .child(label)
}

fn segmented_button(
    theme: Theme,
    label: &'static str,
    id: &'static str,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let fill = if selected {
        with_alpha(theme.accent_blue, 0x64)
    } else {
        0xffffff0c
    };
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .h(px(30.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            theme.edge_highlight,
            if selected { 0x70 } else { 0x24 },
        )))
        .cursor_pointer()
        .bg(rgba(fill))
        .text_color(rgba(if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        }))
        .text_size(px(11.0))
        .child(label)
        .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x22))))
        .on_click(on_click)
}

fn stepper_button(
    theme: Theme,
    label: &'static str,
    id: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .w(px(26.0))
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x30)))
        .cursor_pointer()
        .text_color(rgba(theme.text_primary))
        .text_size(px(15.0))
        .child(label)
        .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x22))))
        .on_click(on_click)
}

#[cfg(test)]
#[path = "../../tests/unit/layout_menu_geometry_tests.rs"]
mod layout_menu_geometry_tests;
