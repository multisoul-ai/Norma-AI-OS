use super::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Selectable,
};

impl MultisoulApp {
    /// Material tonal surfaces keep controls and content on the same neutral palette.
    pub(crate) fn staff_theme(&self) -> ui::Theme {
        let light = self.theme.is_light();
        ui::Theme {
            graphite: if light { 0xf0f4f9ff } else { 0x131314ff },
            panel: if light { 0xffffffff } else { 0x1e1f20ff },
            panel_high: if light { 0xe9eef6ff } else { 0x282a2cff },
            text_primary: if light { 0x202124ff } else { 0xe3e3e3ff },
            text_secondary: if light { 0x626975ff } else { 0xa2a7aeff },
            text_tertiary: if light { 0x747b85ff } else { 0x858b93ff },
            accent_cyan: if light { 0x0b57d0ff } else { 0xa8c7faff },
            accent_warm: if light { 0x845500ff } else { 0xeac282ff },
            accent_violet: if light { 0x536a94ff } else { 0xb9c8e4ff },
            success: if light { 0x267342ff } else { 0x9cd7b2ff },
            ..self.theme
        }
    }

    pub(crate) fn staff_button_style(&self, selected: bool, cx: &gpui::App) -> ButtonCustomVariant {
        let theme = self.staff_theme();
        let tint = ui::icons::with_alpha(theme.accent_cyan, if selected { 44 } else { 0 });
        ButtonCustomVariant::new(cx)
            .color(rgba(tint).into())
            .foreground(
                rgba(if selected {
                    theme.accent_cyan
                } else {
                    theme.text_primary
                })
                .into(),
            )
            .hover(rgba(ui::icons::with_alpha(theme.text_primary, 24)).into())
            .active(rgba(ui::icons::with_alpha(theme.accent_cyan, 60)).into())
    }

    pub(crate) fn staff_primary_style(&self, cx: &gpui::App) -> ButtonCustomVariant {
        let theme = self.staff_theme();
        ButtonCustomVariant::new(cx)
            .color(rgba(theme.accent_cyan).into())
            .foreground(
                rgba(if self.theme.is_light() {
                    0xffffffff
                } else {
                    0x0b2447ff
                })
                .into(),
            )
            .hover(rgba(ui::icons::with_alpha(theme.accent_cyan, 230)).into())
            .active(rgba(ui::icons::with_alpha(theme.accent_cyan, 200)).into())
    }

    pub(crate) fn staff_panel_top(&self) -> f32 {
        crate::app::chrome::TOP_BAR_HEIGHT
    }
}

impl MultisoulApp {
    pub(crate) fn desktop_mode_switch(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().items_center().gap(px(4.));
        for (enabled, id, label) in [
            (false, "desktop-canvas-mode", "staff.canvas"),
            (true, "desktop-staff-toggle", "staff.mode"),
        ] {
            let selected = self.staff.enabled == enabled;
            row = row.child(
                Button::new(id)
                    .debug_selector(move || id.into())
                    .label(ui::i18n::text(label))
                    .h(px(36.))
                    .rounded(px(8.))
                    .outline()
                    .when(self.staff.enabled, |button| {
                        button.custom(self.staff_button_style(selected, cx))
                    })
                    .selected(selected)
                    .disabled(enabled && self.desktop_workspace.active_id.is_none())
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.set_staff_mode(enabled, window, cx)
                    })),
            );
        }
        row
    }
}
