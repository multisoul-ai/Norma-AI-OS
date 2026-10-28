//! Core UI V2 主题 token（颜色使用 `0xRRGGBBAA`）。

#[path = "theme/mineral.rs"]
mod mineral;
#[path = "theme/neutral.rs"]
mod neutral;

/// Stable theme identifiers. `ALL` is the user-selectable catalog; light variants are legacy fixtures.
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
pub enum ThemeId {
    /// Legacy light profile, excluded from the selectable catalog.
    PaperBlue,
    LinenSage,
    SilverViolet,
    #[default]
    InkBlack,
    Graphite,
    WarmObsidian,
    SapphireNight,
    Moonstone,
    DeepPetrol,
}

impl ThemeId {
    pub const ALL: [Self; 6] = [
        Self::InkBlack,
        Self::Graphite,
        Self::WarmObsidian,
        Self::SapphireNight,
        Self::Moonstone,
        Self::DeepPetrol,
    ];

    /// Restore a saved preference, migrating retired palettes to their selected replacements.
    pub fn from_saved_id(saved: &str) -> Option<Self> {
        match saved {
            "paper-blue" | "linen-sage" | "silver-violet" => Some(Self::InkBlack),
            "midnight-violet" => Some(Self::Moonstone),
            "forest-mint" => Some(Self::DeepPetrol),
            _ => Self::ALL.into_iter().find(|id| id.as_str() == saved),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InkBlack => "ink-black",
            Self::Graphite => "graphite",
            Self::WarmObsidian => "warm-obsidian",
            Self::PaperBlue => "paper-blue",
            Self::LinenSage => "linen-sage",
            Self::SilverViolet => "silver-violet",
            Self::SapphireNight => "sapphire-night",
            Self::Moonstone => "moonstone",
            Self::DeepPetrol => "deep-petrol",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::InkBlack => crate::i18n::text("theme.ink_black"),
            Self::Graphite => crate::i18n::text("theme.graphite"),
            Self::WarmObsidian => crate::i18n::text("theme.warm_obsidian"),
            Self::PaperBlue => crate::i18n::text("theme.paper_blue"),
            Self::LinenSage => crate::i18n::text("theme.linen_sage"),
            Self::SilverViolet => crate::i18n::text("theme.silver_violet"),
            Self::SapphireNight => crate::i18n::text("theme.sapphire_night"),
            Self::Moonstone => crate::i18n::text("theme.moonstone"),
            Self::DeepPetrol => crate::i18n::text("theme.deep_petrol"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::InkBlack => crate::i18n::text("theme.ink_black_description"),
            Self::Graphite => crate::i18n::text("theme.graphite_description"),
            Self::WarmObsidian => crate::i18n::text("theme.warm_obsidian_description"),
            Self::PaperBlue => crate::i18n::text("theme.paper_white_cobalt"),
            Self::LinenSage => crate::i18n::text("theme.linen_sage_mint"),
            Self::SilverViolet => crate::i18n::text("theme.silver_graphite_violet"),
            Self::SapphireNight => crate::i18n::text("theme.sapphire_cobalt"),
            Self::Moonstone => crate::i18n::text("theme.moonstone_description"),
            Self::DeepPetrol => crate::i18n::text("theme.deep_petrol_description"),
        }
    }

    pub const fn is_dark(self) -> bool {
        matches!(
            self,
            Self::InkBlack
                | Self::Graphite
                | Self::WarmObsidian
                | Self::SapphireNight
                | Self::Moonstone
                | Self::DeepPetrol
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
    pub const fn is_light(self) -> bool {
        (self.canvas_from >> 24) > 128
    }

    /// A shared reading veil for wallpaper and its frosted samples; dark themes retain the original wallpaper.
    pub const fn wallpaper_veil(self) -> Option<(u32, u32)> {
        if self.is_light() {
            Some((self.canvas_overlay_from, self.canvas_overlay_to))
        } else {
            None
        }
    }

    pub const fn for_id(id: ThemeId) -> Self {
        match id {
            ThemeId::InkBlack => Self::ink_black(),
            ThemeId::Graphite => Self::graphite(),
            ThemeId::WarmObsidian => Self::warm_obsidian(),
            ThemeId::PaperBlue => Self::paper_blue(),
            ThemeId::LinenSage => Self::linen_sage(),
            ThemeId::SilverViolet => Self::silver_violet(),
            ThemeId::SapphireNight => Self::sapphire_night(),
            ThemeId::Moonstone => Self::moonstone(),
            ThemeId::DeepPetrol => Self::deep_petrol(),
        }
    }

    pub const fn paper_blue() -> Self {
        Self {
            graphite: 0x202938ff,
            panel: 0xf6f8fced,
            panel_high: 0xfffffffa,
            canvas_from: 0xf6f8fcff,
            canvas_to: 0xeaf0f8ff,
            canvas_overlay_from: 0xeaf0f6d9,
            canvas_overlay_to: 0xeff6ffbd,
            node_fill: 0xf6f8fcf0,
            node_stroke: 0xb8c5d699,
            selected_stroke: 0x2563ebc8,
            focused_stroke: 0x2563ebff,
            edge_highlight: 0xffffffcc,
            text_primary: 0x202938ff,
            text_secondary: 0x4b5a70ff,
            text_tertiary: 0x637086ff,
            accent: 0x2563ebff,
            success: 0x16784bff,
            accent_orange: 0x2563ebff,
            accent_cyan: 0x086b80ff,
            accent_green: 0x16784bff,
            accent_violet: 0x7054cfff,
            accent_blue: 0x2563ebff,
            accent_warm: 0x8a6414ff,
            destructive: 0xbb3547ff,
            term_fg: 0x28364aff,
            term_bg: 0xfafcfff8,
            term_exit_fg: 0xbb3547ff,
            title_bar: 0xeaf0f8fa,
            text: 0x202938ff,
            dock: 0xf6f8fcf5,
            dock_line: 0xb8c5d699,
            shadow_color: 0x20355220,
            shadow_y: 22.0,
            shadow_blur: 54.0,
            shadow_spread: -18.0,
        }
    }

    pub const fn linen_sage() -> Self {
        Self {
            graphite: 0x29372fff,
            panel: 0xf8f7f2ed,
            panel_high: 0xfffefafa,
            canvas_from: 0xf8f7f2ff,
            canvas_to: 0xeeefe6ff,
            canvas_overlay_from: 0xf2f2e6d9,
            canvas_overlay_to: 0xe7eeddbd,
            node_fill: 0xf8f7f2f0,
            node_stroke: 0xbac7bb99,
            selected_stroke: 0x42745ac8,
            focused_stroke: 0x42745aff,
            edge_highlight: 0xfffefacc,
            text_primary: 0x29372fff,
            text_secondary: 0x506256ff,
            text_tertiary: 0x637166ff,
            accent: 0x42745aff,
            success: 0x247147ff,
            accent_orange: 0x42745aff,
            accent_cyan: 0x246e71ff,
            accent_green: 0x247147ff,
            accent_violet: 0x785d9dff,
            accent_blue: 0x42745aff,
            accent_warm: 0x86611fff,
            destructive: 0xb13f3fff,
            term_fg: 0x29372fff,
            term_bg: 0xfdfcf7f8,
            term_exit_fg: 0xb13f3fff,
            title_bar: 0xeeefe6fa,
            text: 0x29372fff,
            dock: 0xf8f7f2f5,
            dock_line: 0xbac7bb99,
            shadow_color: 0x34453220,
            shadow_y: 22.0,
            shadow_blur: 54.0,
            shadow_spread: -18.0,
        }
    }

    pub const fn silver_violet() -> Self {
        Self {
            graphite: 0x2f2d3cff,
            panel: 0xf7f7fbed,
            panel_high: 0xfefefffa,
            canvas_from: 0xf7f7fbff,
            canvas_to: 0xeeeef6ff,
            canvas_overlay_from: 0xeeecf6d9,
            canvas_overlay_to: 0xede9f8bd,
            node_fill: 0xf7f7fbf0,
            node_stroke: 0xc7c0d899,
            selected_stroke: 0x7054cfc8,
            focused_stroke: 0x7054cfff,
            edge_highlight: 0xfefeffcc,
            text_primary: 0x2f2d3cff,
            text_secondary: 0x5a546cff,
            text_tertiary: 0x706a80ff,
            accent: 0x7054cfff,
            success: 0x347651ff,
            accent_orange: 0x7054cfff,
            accent_cyan: 0x386e85ff,
            accent_green: 0x347651ff,
            accent_violet: 0x7054cfff,
            accent_blue: 0x7054cfff,
            accent_warm: 0x85601cff,
            destructive: 0xb63853ff,
            term_fg: 0x302d42ff,
            term_bg: 0xfcfbfff8,
            term_exit_fg: 0xb63853ff,
            title_bar: 0xeeeef6fa,
            text: 0x2f2d3cff,
            dock: 0xf7f7fbf5,
            dock_line: 0xc7c0d899,
            shadow_color: 0x42335c20,
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

#[cfg(test)]
#[path = "../tests/unit/light_theme_material_tests.rs"]
mod light_theme_material_tests;
