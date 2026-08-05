//! Decorative elements for the anchored queue sidecar.

use gpui::{div, prelude::*, px, rgba, Div};
use ui::Theme;

pub(super) fn anchor_pointer(panel_on_right: bool, theme: Theme) -> Div {
    let glyph = if panel_on_right { "◀" } else { "▶" };
    div()
        .absolute()
        .top(px(24.0))
        .when(panel_on_right, |pointer| pointer.left(px(-10.0)))
        .when(!panel_on_right, |pointer| pointer.right(px(-10.0)))
        .text_size(px(20.0))
        .text_color(rgba(with_alpha(theme.panel, 0xf4)))
        .child(glyph)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
