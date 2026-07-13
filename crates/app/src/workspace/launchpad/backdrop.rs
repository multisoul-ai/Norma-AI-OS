use super::with_alpha;
use gpui::{div, linear_color_stop, linear_gradient, prelude::*, px, rgba, Div};
use ui::Theme;

/// Build the launchpad backdrop from its dedicated video layer. The caller
/// never passes the canvas wallpaper layer here, keeping Launchpad's home.mp4
/// independent from the user's Canvas wallpaper setting.
pub(super) fn launchpad_backdrop(theme: Theme, video: Div) -> Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .overflow_hidden()
        .bg(rgba(theme.graphite))
        .child(video)
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0xd6)), 0.0),
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0x68)), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0x40)), 0.0),
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0xb8)), 1.0),
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
                .border_color(rgba(with_alpha(theme.accent_cyan, 0x28))),
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
                .border_color(rgba(with_alpha(theme.accent_orange, 0x30))),
        )
        .child(
            div()
                .absolute()
                .left(px(90.0))
                .right(px(90.0))
                .bottom(px(84.0))
                .h(px(1.0))
                .bg(rgba(with_alpha(theme.accent_cyan, 0x22))),
        )
}
