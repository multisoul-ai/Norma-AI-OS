use super::MultisoulApp;

impl MultisoulApp {
    /// Staff is a neutral reading surface, matching the prototype across wallpaper themes.
    pub(crate) fn staff_theme(&self) -> ui::Theme {
        ui::Theme {
            graphite: 0x16171cff,
            panel: 0x14151aff,
            panel_high: 0x2b2e38ff,
            text_primary: 0xeeeff2ff,
            text_secondary: 0x95969fff,
            text_tertiary: 0x686a75ff,
            accent_cyan: 0x9ccbd4ff,
            accent_warm: 0xdbc6a1ff,
            accent_violet: 0xc2b2dbff,
            success: 0x91bba9ff,
            ..self.theme
        }
    }

    pub(crate) fn staff_panel_top(&self) -> f32 {
        if self.staff.fullscreen {
            0.
        } else {
            crate::app::chrome::TOP_BAR_HEIGHT
        }
    }
}
