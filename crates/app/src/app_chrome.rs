//! Workspace overlay chrome for the GPUI app shell.

use super::{
    MultisoulApp, WALLPAPER_IMAGE_OPACITY, WALLPAPER_SHADE_BOTTOM_ALPHA, WALLPAPER_SHADE_TOP_ALPHA,
};
use gpui::{
    div, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    Div, ObjectFit, Window,
};
use ui::Theme;

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
        let bottom_w = (win_w - 64.0).clamp(260.0, 360.0);
        let bottom_left = (win_w - bottom_w) * 0.5;

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .child(
                div()
                    .absolute()
                    .top(px(30.0))
                    .left(px((win_w - 312.0) * 0.5))
                    .w(px(312.0))
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
                    .child(chrome_button(theme, "v", false))
                    .child(div().px(px(8.0)).child("CANVAS"))
                    .child(
                        chrome_button(theme, "+", true)
                            .id("toolbar-new-terminal")
                            .on_click(cx.listener(Self::on_toolbar_new_terminal)),
                    )
                    .child(
                        chrome_button(theme, "CL", false)
                            .id("toolbar-new-claude-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_claude_agent)),
                    )
                    .child(
                        chrome_button(theme, "CX", false)
                            .id("toolbar-new-codex-agent")
                            .on_click(cx.listener(Self::on_toolbar_new_codex_agent)),
                    )
                    .child(
                        chrome_button(theme, "[]", false)
                            .id("toolbar-reflow")
                            .on_click(cx.listener(Self::on_toolbar_reflow)),
                    )
                    .child(chrome_button(theme, "*", false)),
            )
            .child(
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
            .child(
                div()
                    .absolute()
                    .right(px(24.0))
                    .top(px(34.0))
                    .h(px(38.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px(px(14.0))
                    .rounded(px(19.0))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(theme.accent_orange))
                    .text_color(rgba(0xffffffff))
                    .text_size(px(12.0))
                    .child(chrome_button(theme, "!", true))
                    .child("Feedback")
                    .id("workspace-feedback")
                    .on_click(cx.listener(Self::on_feedback_click)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(bottom_left))
                    .bottom(px(22.0))
                    .w(px(bottom_w))
                    .h(px(42.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(18.0))
                    .rounded(px(21.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x2d)))
                    .shadow(chrome_shadow(theme))
                    .bg(rgba(with_alpha(theme.panel, 0xe5)))
                    .text_color(rgba(theme.text_primary))
                    .text_size(px(13.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(18.0))
                                    .h(px(18.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgba(theme.accent_orange))
                                    .child("|||"),
                            )
                            .child("Listening..."),
                    )
                    .child(
                        div()
                            .text_color(rgba(theme.accent_orange))
                            .child("........"),
                    ),
            )
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
        .w(px(28.0))
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

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
