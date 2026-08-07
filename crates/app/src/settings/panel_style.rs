//! Shared Settings panel style helpers.
//!
//! Theme colors are `0xRRGGBBAA` (alpha in the low byte). Keep `with_alpha` in
//! that layout — swapping to AARRGGBB turns panel fills into bright red.

use gpui::{point, px, rgba, BoxShadow};
use ui::Theme;

pub(super) fn settings_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(with_alpha(theme.shadow_color, 0xb0)).into(),
        offset: point(px(0.0), px(32.0)),
        blur_radius: px(72.0),
        spread_radius: px(-24.0),
    }]
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
