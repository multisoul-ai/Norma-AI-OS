use gpui::{px, rgba, BoxShadow};
use ui::Theme;

pub(super) fn guide_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(with_alpha(theme.shadow_color, 0x78)).into(),
        offset: gpui::point(px(0.0), px(22.0)),
        blur_radius: px(60.0),
        spread_radius: px(-16.0),
    }]
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
