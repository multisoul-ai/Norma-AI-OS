use gpui::{px, rgba, BoxShadow};
use ui::Theme;

const MIN_SMALL_TEXT_CONTRAST: f32 = 4.5;
const DARK_ON_ACCENT: u32 = 0x000000ff;
const LIGHT_ON_ACCENT: u32 = 0xffffffff;

#[derive(Clone, Copy, Debug)]
pub(super) struct FeatureGuidePalette {
    pub(super) primary_text: u32,
    pub(super) supporting_text: u32,
    pub(super) metadata_text: u32,
    pub(super) accent_text: u32,
    pub(super) status_text: u32,
    pub(super) primary_background: u32,
    pub(super) primary_hover: u32,
    pub(super) primary_active: u32,
    pub(super) primary_foreground: u32,
}

impl FeatureGuidePalette {
    pub(super) fn from_theme(theme: Theme) -> Self {
        let panel = opaque(theme.panel_high);
        let primary_text = opaque(theme.text_primary);
        let supporting_text = readable_text_mix(primary_text, panel, 0.78);
        let metadata_text = readable_text_mix(primary_text, panel, 0.72);
        let accent_text = readable_tint(theme.accent_blue, theme);
        let primary_background = opaque(theme.accent_blue);
        let primary_foreground = best_on_accent(primary_background);
        let interaction_target = if primary_foreground == DARK_ON_ACCENT {
            LIGHT_ON_ACCENT
        } else {
            DARK_ON_ACCENT
        };

        Self {
            primary_text,
            supporting_text,
            metadata_text,
            accent_text,
            status_text: supporting_text,
            primary_background,
            primary_hover: mix_colors(primary_background, interaction_target, 0.08),
            primary_active: mix_colors(primary_background, interaction_target, 0.14),
            primary_foreground,
        }
    }
}

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

pub(super) fn readable_tint(tint: u32, theme: Theme) -> u32 {
    ensure_contrast(
        opaque(tint),
        opaque(theme.text_primary),
        opaque(theme.panel_high),
    )
}

fn readable_text_mix(foreground: u32, background: u32, amount: f32) -> u32 {
    ensure_contrast(
        mix_colors(background, foreground, amount),
        foreground,
        background,
    )
}

fn ensure_contrast(candidate: u32, fallback: u32, background: u32) -> u32 {
    if contrast_ratio(candidate, background) >= MIN_SMALL_TEXT_CONTRAST {
        return candidate;
    }
    for step in 1..=16 {
        let adjusted = mix_colors(candidate, fallback, step as f32 / 16.0);
        if contrast_ratio(adjusted, background) >= MIN_SMALL_TEXT_CONTRAST {
            return adjusted;
        }
    }
    fallback
}

fn best_on_accent(background: u32) -> u32 {
    let dark_contrast = contrast_ratio(DARK_ON_ACCENT, background);
    let light_contrast = contrast_ratio(LIGHT_ON_ACCENT, background);
    if dark_contrast >= light_contrast {
        DARK_ON_ACCENT
    } else {
        LIGHT_ON_ACCENT
    }
}

fn mix_colors(from: u32, to: u32, amount: f32) -> u32 {
    let from = rgb_channels(from);
    let to = rgb_channels(to);
    pack_rgb([
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
    ])
}

fn contrast_ratio(foreground: u32, background: u32) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn relative_luminance(color: u32) -> f32 {
    let linear = rgb_channels(color).map(|channel| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

fn rgb_channels(color: u32) -> [f32; 3] {
    [
        ((color >> 24) & 0xff) as f32 / 255.0,
        ((color >> 16) & 0xff) as f32 / 255.0,
        ((color >> 8) & 0xff) as f32 / 255.0,
    ]
}

fn pack_rgb(channels: [f32; 3]) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(channels[0]) << 24) | (channel(channels[1]) << 16) | (channel(channels[2]) << 8) | 0xff
}

fn opaque(color: u32) -> u32 {
    with_alpha(color, 0xff)
}
