use super::with_alpha;
use gpui::{div, linear_color_stop, linear_gradient, prelude::*, px, rgba, Div};
use ui::Theme;

/// Build the launchpad backdrop: the wallpaper layer is passed in pre-built by
/// `render_launchpad` (via the canvas's shared `render_wallpaper_layer`), so it
/// shares the canvas's video-aware render path — a live `surface()` for Video,
/// the static image element with a source-derived id for Static/Gif. Receiving
/// the wallpaper as an already-built `Div` (rather than as a raw image source)
/// is what lets mp4/mov render here exactly as on the canvas.
pub(super) fn launchpad_backdrop(theme: Theme, wallpaper: Div) -> Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .overflow_hidden()
        .bg(rgba(theme.graphite))
        .child(wallpaper)
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
