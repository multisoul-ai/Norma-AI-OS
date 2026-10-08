//! Core UI V2 主题 token（颜色使用 `0xRRGGBBAA`）。

/// 用户可选的稳定主题标识。
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
pub enum ThemeId {
    PaperBlue,
    LinenSage,
    SilverViolet,
    #[default]
    SapphireNight,
    MidnightViolet,
    ForestMint,
}

impl ThemeId {
    pub const ALL: [Self; 6] = [
        Self::PaperBlue,
        Self::LinenSage,
        Self::SilverViolet,
        Self::SapphireNight,
        Self::MidnightViolet,
        Self::ForestMint,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PaperBlue => "paper-blue",
            Self::LinenSage => "linen-sage",
            Self::SilverViolet => "silver-violet",
            Self::SapphireNight => "sapphire-night",
            Self::MidnightViolet => "midnight-violet",
            Self::ForestMint => "forest-mint",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::PaperBlue => crate::i18n::text("theme.paper_blue"),
            Self::LinenSage => crate::i18n::text("theme.linen_sage"),
            Self::SilverViolet => crate::i18n::text("theme.silver_violet"),
            Self::SapphireNight => crate::i18n::text("theme.sapphire_night"),
            Self::MidnightViolet => crate::i18n::text("theme.midnight_violet"),
            Self::ForestMint => crate::i18n::text("theme.forest_mint"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::PaperBlue => crate::i18n::text("theme.paper_white_cobalt"),
            Self::LinenSage => crate::i18n::text("theme.linen_sage_mint"),
            Self::SilverViolet => crate::i18n::text("theme.silver_graphite_violet"),
            Self::SapphireNight => crate::i18n::text("theme.sapphire_cobalt"),
            Self::MidnightViolet => crate::i18n::text("theme.midnight_violet_description"),
            Self::ForestMint => crate::i18n::text("theme.deep_sage_mint"),
        }
    }

    pub const fn is_dark(self) -> bool {
        matches!(
            self,
            Self::SapphireNight | Self::MidnightViolet | Self::ForestMint
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub graphite: u32,
    pub panel: u32,
    pub panel_high: u32,
    pub canvas_from: u32,
    pub canvas_to: u32,
    pub canvas_overlay_from: u32,
    pub canvas_overlay_to: u32,
    pub node_fill: u32,
    pub node_stroke: u32,
    pub selected_stroke: u32,
    pub focused_stroke: u32,
    pub edge_highlight: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_tertiary: u32,
    pub accent: u32,
    pub success: u32,
    pub accent_orange: u32,
    pub accent_cyan: u32,
    pub accent_green: u32,
    pub accent_violet: u32,
    pub accent_blue: u32,
    pub accent_warm: u32,
    pub destructive: u32,
    pub term_fg: u32,
    pub term_bg: u32,
    pub term_exit_fg: u32,
    pub title_bar: u32,
    pub text: u32,
    pub dock: u32,
    pub dock_line: u32,
    pub shadow_color: u32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
}

impl Theme {
    pub const fn for_id(id: ThemeId) -> Self {
        match id {
            ThemeId::PaperBlue => Self::paper_blue(),
            ThemeId::LinenSage => Self::linen_sage(),
            ThemeId::SilverViolet => Self::silver_violet(),
            ThemeId::SapphireNight => Self::sapphire_night(),
            ThemeId::MidnightViolet => Self::midnight_violet(),
            ThemeId::ForestMint => Self::forest_mint(),
        }
    }

    pub const fn paper_blue() -> Self {
        Self {
            graphite: 0x313d44ff,
            panel: 0xffffff66,
            panel_high: 0xfffefbd1,
            canvas_from: 0xfffefbff,
            canvas_to: 0xf5f4f1ff,
            canvas_overlay_from: 0xfffefbeb,
            canvas_overlay_to: 0xd4eaf7d6,
            node_fill: 0xffffff66,
            node_stroke: 0xffffffb8,
            selected_stroke: 0x2f80edc8,
            focused_stroke: 0x2f80edff,
            edge_highlight: 0xfffffff0,
            text_primary: 0x313d44ff,
            text_secondary: 0x313d4494,
            text_tertiary: 0x313d446b,
            accent: 0x2f80edff,
            success: 0x64e0b5ff,
            accent_orange: 0x2f80edff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x64e0b5ff,
            accent_violet: 0x6a5cffff,
            accent_blue: 0x2f80edff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x0e151deb,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0xd4eaf752,
            text: 0x313d44ff,
            dock: 0x313d44cc,
            dock_line: 0xffffff57,
            shadow_color: 0x313d442b,
            shadow_y: 22.0,
            shadow_blur: 54.0,
            shadow_spread: -18.0,
        }
    }

    pub const fn linen_sage() -> Self {
        Self {
            graphite: 0x1d231fff,
            panel: 0xfafcf869,
            panel_high: 0xfafbf7d1,
            canvas_from: 0xf9f8f3ff,
            canvas_to: 0xebede6ff,
            canvas_overlay_from: 0xf9f8f3eb,
            canvas_overlay_to: 0xcdd8ced9,
            node_fill: 0xfafcf869,
            node_stroke: 0xffffffe0,
            selected_stroke: 0x3d9978c8,
            focused_stroke: 0x3d9978ff,
            edge_highlight: 0xffffffe8,
            text_primary: 0x1d231fff,
            text_secondary: 0x1d231f94,
            text_tertiary: 0x1d231f6b,
            accent: 0x3d9978ff,
            success: 0x64e0b5ff,
            accent_orange: 0x3d9978ff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x64e0b5ff,
            accent_violet: 0x6a5cffff,
            accent_blue: 0x3d9978ff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x0e151deb,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0xcdd8ce57,
            text: 0x1d231fff,
            dock: 0x2a302cd1,
            dock_line: 0xffffff57,
            shadow_color: 0x37463d2b,
            shadow_y: 22.0,
            shadow_blur: 54.0,
            shadow_spread: -18.0,
        }
    }

    pub const fn silver_violet() -> Self {
        Self {
            graphite: 0x232328ff,
            panel: 0xfcfcfd69,
            panel_high: 0xfcfcfcd1,
            canvas_from: 0xfafaf9ff,
            canvas_to: 0xf0f0f2ff,
            canvas_overlay_from: 0xfafaf9ed,
            canvas_overlay_to: 0xdcdde1db,
            node_fill: 0xfcfcfd69,
            node_stroke: 0xffffffe0,
            selected_stroke: 0x6a5cffc8,
            focused_stroke: 0x6a5cffff,
            edge_highlight: 0xffffffe8,
            text_primary: 0x232328ff,
            text_secondary: 0x23232891,
            text_tertiary: 0x23232869,
            accent: 0x6a5cffff,
            success: 0x64e0b5ff,
            accent_orange: 0x6a5cffff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x64e0b5ff,
            accent_violet: 0x6a5cffff,
            accent_blue: 0x6a5cffff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x0e151deb,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0xe2e1e85c,
            text: 0x232328ff,
            dock: 0x2a2a2fd1,
            dock_line: 0xffffff57,
            shadow_color: 0x34333d2b,
            shadow_y: 22.0,
            shadow_blur: 54.0,
            shadow_spread: -18.0,
        }
    }

    pub const fn sapphire_night() -> Self {
        Self {
            graphite: 0x071324ff,
            panel: 0x0e23385c,
            panel_high: 0x0b1d2eb3,
            canvas_from: 0x071324ff,
            canvas_to: 0x0e2338ff,
            canvas_overlay_from: 0x071324c7,
            canvas_overlay_to: 0x0a1626c2,
            node_fill: 0x0e23385c,
            node_stroke: 0xafdaff4d,
            selected_stroke: 0x54a2ffcc,
            focused_stroke: 0x54a2ffff,
            edge_highlight: 0xceeaff94,
            text_primary: 0xeff7ffff,
            text_secondary: 0xdcebf999,
            text_tertiary: 0xcbdced6e,
            accent: 0x54a2ffff,
            success: 0x64e0b5ff,
            accent_orange: 0x54a2ffff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x64e0b5ff,
            accent_violet: 0x8b7cffff,
            accent_blue: 0x54a2ffff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x030c16d1,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0x1831486b,
            text: 0xeff7ffff,
            dock: 0x040e1ac2,
            dock_line: 0xafdaff4d,
            shadow_color: 0x0006107a,
            shadow_y: 20.0,
            shadow_blur: 48.0,
            shadow_spread: -20.0,
        }
    }

    pub const fn midnight_violet() -> Self {
        Self {
            graphite: 0x0c0c1bff,
            panel: 0x231f3c61,
            panel_high: 0x19162db8,
            canvas_from: 0x0c0c1bff,
            canvas_to: 0x231f3cff,
            canvas_overlay_from: 0x0c0c1bd1,
            canvas_overlay_to: 0x23183ac2,
            node_fill: 0x231f3c61,
            node_stroke: 0xc4b8ff4d,
            selected_stroke: 0x8b7cffcc,
            focused_stroke: 0x8b7cffff,
            edge_highlight: 0xdcd3ff8f,
            text_primary: 0xf8f5ffff,
            text_secondary: 0xe4dcf599,
            text_tertiary: 0xd3c9e86e,
            accent: 0x8b7cffff,
            success: 0x64e0b5ff,
            accent_orange: 0x8b7cffff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x64e0b5ff,
            accent_violet: 0x8b7cffff,
            accent_blue: 0x8b7cffff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x070713d6,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0x342d526b,
            text: 0xf8f5ffff,
            dock: 0x0a0818c7,
            dock_line: 0xc4b8ff4d,
            shadow_color: 0x02010c80,
            shadow_y: 20.0,
            shadow_blur: 48.0,
            shadow_spread: -20.0,
        }
    }

    pub const fn forest_mint() -> Self {
        Self {
            graphite: 0x091716ff,
            panel: 0x112d2761,
            panel_high: 0x0e2621b8,
            canvas_from: 0x091716ff,
            canvas_to: 0x112d27ff,
            canvas_overlay_from: 0x091716d1,
            canvas_overlay_to: 0x151f1bc7,
            node_fill: 0x112d2761,
            node_stroke: 0x9de4cd4a,
            selected_stroke: 0x64e0b5cc,
            focused_stroke: 0x64e0b5ff,
            edge_highlight: 0xc7f4e48c,
            text_primary: 0xf1fcf8ff,
            text_secondary: 0xd2ebe299,
            text_tertiary: 0xc1dcd26e,
            accent: 0x64e0b5ff,
            success: 0xd6ff3bff,
            accent_orange: 0x64e0b5ff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0xd6ff3bff,
            accent_violet: 0x8b7cffff,
            accent_blue: 0x64e0b5ff,
            accent_warm: 0xd6ff3bff,
            destructive: 0xff6b7aff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x04110ed6,
            term_exit_fg: 0xff6b7aff,
            title_bar: 0x1d3d356e,
            text: 0xf1fcf8ff,
            dock: 0x051410c7,
            dock_line: 0x9de4cd4a,
            shadow_color: 0x010c0980,
            shadow_y: 20.0,
            shadow_blur: 48.0,
            shadow_spread: -20.0,
        }
    }

    pub const fn glass_light() -> Self {
        Self::paper_blue()
    }

    pub const fn glass_dark() -> Self {
        Self {
            graphite: 0x05090bff,
            panel: 0x0e1b16cc,
            panel_high: 0x1f372ab8,
            canvas_from: 0x101722f2,
            canvas_to: 0x1d2635e6,
            canvas_overlay_from: 0x101722f2,
            canvas_overlay_to: 0x1d2635e6,
            node_fill: 0x0e1b16cc,
            node_stroke: 0xffffff24,
            selected_stroke: 0xff7932ff,
            focused_stroke: 0x27d7eaff,
            edge_highlight: 0xffffff3d,
            text_primary: 0xfffffff5,
            text_secondary: 0xffffffad,
            text_tertiary: 0xffffff75,
            accent: 0xff7932ff,
            success: 0x60f078ff,
            accent_orange: 0xff7932ff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x60f078ff,
            accent_violet: 0x9b55ffff,
            accent_blue: 0x4b70ffff,
            accent_warm: 0xf2c26bff,
            destructive: 0xff3c4dff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x080d13fa,
            term_exit_fg: 0xff6961ff,
            title_bar: 0x13231dc9,
            text: 0xf2f6fbff,
            dock: 0x05090bcc,
            dock_line: 0xffffff3d,
            shadow_color: 0x00000073,
            shadow_y: 20.0,
            shadow_blur: 48.0,
            shadow_spread: -20.0,
        }
    }

    pub const fn dark() -> Self {
        Self::glass_dark()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::for_id(ThemeId::default())
    }
}

#[cfg(test)]
#[path = "../tests/unit/product_ui_theme_tests.rs"]
mod product_ui_theme_tests;

#[cfg(test)]
#[path = "../tests/unit/theme_catalog_tests.rs"]
mod theme_catalog_tests;
