//! Desktop chrome shares the selected theme; terminal/Dock content keeps its own material.
//! Apple material hierarchy: dense translucent reading surfaces, semantic text and immediate feedback.
use crate::app::MultisoulApp;
use ui::icons::with_alpha;

pub(super) fn is_light(theme: ui::Theme) -> bool {
    ((theme.canvas_from >> 24) & 0xff) > 128
}

pub(super) fn mix(base: u32, ink: u32, amount: u32) -> u32 {
    let mut color = 0xff;
    for shift in [24, 16, 8] {
        let a = (base >> shift) & 0xff;
        let b = (ink >> shift) & 0xff;
        color |= ((a * (255 - amount) + b * amount) / 255) << shift;
    }
    color
}

/// The six catalog themes remain unchanged. Only wallpaper-facing chrome gets denser materials.
pub(crate) fn surface_theme(mut theme: ui::Theme) -> ui::Theme {
    let base = theme.canvas_from;
    theme.panel = with_alpha(base, 0xeb);
    theme.panel_high = with_alpha(mix(base, theme.text_primary, 22), 0xf5);
    theme.node_fill = with_alpha(mix(base, theme.text_primary, 38), 0xfa);
    theme.text_secondary = mix(base, theme.text_primary, 220);
    theme.text_tertiary = mix(base, theme.text_primary, 200);
    theme.node_stroke = with_alpha(theme.text_primary, 0x38);
    theme.edge_highlight = theme.text_primary;
    theme.dock = theme.panel;
    theme.dock_line = theme.node_stroke;
    theme.title_bar = theme.panel_high;
    // Semantic status inks need darker values on light materials, including small status labels.
    if is_light(theme) {
        theme.panel_high = with_alpha(mix(base, 0xffffffff, 150), 0xfa);
        theme.node_fill = with_alpha(mix(base, theme.text_primary, 16), 0xf5);
        theme.title_bar = theme.panel_high;
    }
    theme
}

/// Light materials use the theme's restrained shadow; dark materials retain their depth.
pub(crate) fn shadow_color(theme: ui::Theme, dark_alpha: u8) -> u32 {
    if theme.is_light() {
        theme.shadow_color
    } else {
        with_alpha(0x000000ff, dark_alpha)
    }
}

/// Use relative luminance to choose a readable ink for filled semantic controls.
pub(crate) fn on_color(color: u32) -> u32 {
    let linear = |shift: u32| {
        let channel = ((color >> shift) & 255_u32) as f32 / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(24) + 0.7152 * linear(16) + 0.0722 * linear(8);
    if luminance > 0.179 {
        0x000000ff
    } else {
        0xffffffff
    }
}

pub(super) fn on_accent(theme: ui::Theme) -> u32 {
    on_color(theme.accent)
}

/// Move the fill away from its foreground so hover/pressed retain contrast.
pub(super) fn accent_fill(theme: ui::Theme, amount: u32) -> u32 {
    let target = if on_accent(theme) == 0xffffffff {
        0x000000ff
    } else {
        0xffffffff
    };
    mix(theme.accent, target, amount)
}

impl MultisoulApp {
    pub(super) fn desktop_theme(&self) -> ui::Theme {
        surface_theme(self.theme)
    }

    pub(super) fn desktop_input_fill(&self) -> u32 {
        self.desktop_theme().panel_high
    }

    pub(super) fn sync_desktop_input_theme(&self, _expanded: bool, cx: &mut gpui::Context<Self>) {
        let foreground: gpui::Hsla = gpui::rgba(self.theme.text_primary).into();
        if ui::component::Theme::global(cx).colors.foreground != foreground {
            ui::sync_component_theme(self.theme_id, cx);
        }
    }
}
