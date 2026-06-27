//! Workspace overlay chrome for the GPUI app shell.

use super::{
    MultisoulApp, WALLPAPER_IMAGE_OPACITY, WALLPAPER_SHADE_BOTTOM_ALPHA, WALLPAPER_SHADE_TOP_ALPHA,
};
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Window,
};
use ui::Theme;

const WORKSPACE_LEFT_RAIL_VISIBLE: bool = false;
const BOTTOM_VOICE_BAR_VISIBLE: bool = true;
const TOP_TOOLBAR_WIDTH: f32 = 460.0;
const TOOLBAR_CANVAS_SCROLL_ICON: &str = "↕";
const SETTINGS_ICON: &str = "⚙";

impl MultisoulApp {
    pub(super) fn render_workspace_backdrop(&self) -> Div {
        let theme = self.theme;
        let mut backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .overflow_hidden()
            .bg(linear_gradient(
                150.0,
                linear_color_stop(rgba(theme.canvas_from), 0.0),
                linear_color_stop(rgba(theme.graphite), 1.0),
            ));

        backdrop = backdrop.child(
            img(self.wallpaper.image_source())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .opacity(WALLPAPER_IMAGE_OPACITY),
        );

        backdrop
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_TOP_ALPHA)),
                            0.0,
                        ),
                        linear_color_stop(
                            rgba(with_alpha(theme.graphite, WALLPAPER_SHADE_BOTTOM_ALPHA)),
                            1.0,
                        ),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-160.0))
                    .top(px(54.0))
                    .w(px(520.0))
                    .h(px(520.0))
                    .rounded(px(260.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_cyan, 0x24))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(-180.0))
                    .bottom(px(-120.0))
                    .w(px(620.0))
                    .h(px(620.0))
                    .rounded(px(310.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_orange, 0x2e))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(90.0))
                    .right(px(90.0))
                    .bottom(px(84.0))
                    .h(px(1.0))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x18))),
            )
    }

    pub(super) fn render_workspace_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);

        let mut chrome = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(
                div()
                    .absolute()
                    .top(px(30.0))
                    .left(px((win_w - TOP_TOOLBAR_WIDTH) * 0.5))
                    .w(px(TOP_TOOLBAR_WIDTH))
                    .h(px(46.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px(px(12.0))
                    .rounded(px(23.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xdc)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(12.0))
                    .child(
                        chrome_button(
                            theme,
                            TOOLBAR_CANVAS_SCROLL_ICON,
                            self.interaction.canvas_scroll_enabled,
                        )
                        .id("toolbar-canvas-scroll")
                        .on_click(cx.listener(Self::on_toolbar_canvas_scroll)),
                    )
                    .child(div().px(px(8.0)).child("CANVAS"))
                    .child(
                        chrome_button(theme, "+", true)
                            .id("toolbar-new-terminal")
                            .on_click(cx.listener(Self::on_toolbar_new_terminal)),
                    )
                    .child(
                        chrome_button(theme, "Claude Code", false)
                            .id("toolbar-new-claude-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_claude_agent)),
                    )
                    .child(
                        chrome_button(theme, "Codex", false)
                            .id("toolbar-new-codex-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_codex_agent)),
                    )
                    .child(
                        chrome_button(theme, "Preview", false)
                            .id("toolbar-preview-file")
                            .on_click(cx.listener(Self::on_toolbar_preview_file)),
                    )
                    .child(
                        chrome_button(theme, SETTINGS_ICON, false)
                            .id("toolbar-settings")
                            .on_click(cx.listener(Self::on_toolbar_settings)),
                    ),
            )
            .when(workspace_left_rail_visible(), |chrome| {
                chrome.child(
                    div()
                        .absolute()
                        .left(px(24.0))
                        .top(px(178.0))
                        .w(px(46.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p(px(8.0))
                        .rounded(px(23.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x2f)))
                        .shadow(chrome_shadow(theme))
                        .bg(rgba(with_alpha(theme.panel, 0xd0)))
                        .child(chrome_button(theme, ">", true))
                        .child(chrome_button(theme, "H", false))
                        .child(chrome_button(theme, "[]", false))
                        .child(chrome_button(theme, "<>", false))
                        .child(chrome_button(theme, "O", false))
                        .child(chrome_button(theme, "T", false))
                        .child(chrome_button(theme, "AI", false))
                        .child(chrome_button(theme, "{}", false)),
                )
            });

        if bottom_voice_bar_visible() {
            chrome = chrome.child(self.render_voice_bar(window, cx));
        }

        chrome
    }
}

fn chrome_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(18.0)),
        blur_radius: px(36.0),
        spread_radius: px(-18.0),
    }]
}

fn chrome_button(theme: Theme, label: &'static str, active: bool) -> Div {
    let fill = if active {
        with_alpha(theme.accent_orange, 0x36)
    } else {
        with_alpha(theme.edge_highlight, 0x08)
    };
    let stroke = if active {
        with_alpha(theme.accent_orange, 0x78)
    } else {
        with_alpha(theme.edge_highlight, 0x20)
    };
    let text = if active {
        theme.accent_orange
    } else {
        theme.text_primary
    };

    div()
        .w(px(chrome_button_width(label)))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_color(rgba(text))
        .text_size(px(11.0))
        .child(label)
}

fn chrome_button_width(label: &str) -> f32 {
    let text_width = label.chars().count() as f32 * 6.4;
    (text_width + 18.0).max(28.0).ceil()
}

fn workspace_left_rail_visible() -> bool {
    WORKSPACE_LEFT_RAIL_VISIBLE
}

fn bottom_voice_bar_visible() -> bool {
    BOTTOM_VOICE_BAR_VISIBLE
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/app_chrome_tests.rs"]
mod tests;
