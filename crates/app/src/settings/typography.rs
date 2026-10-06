//! Shared Settings typography scale.
//!
//! Every text role in the Settings panel maps to exactly one size, weight,
//! line height, and default tone, so pages cannot drift into ad-hoc variants.
//! Values follow `docs/design.md` §4.2 and are tuned for a roomy panel:
//! one 18px title size, one 14px headline size, and a 12px body.
//! Call sites may still override the color for semantic states (error,
//! success, accent) after applying a role; size and weight stay owned here.
//! Keep localized letter case and the inherited platform system font. Symbols
//! and avatar initials are artwork, so their sizing remains independent.

use gpui::{div, px, rgba, Div, FontWeight, ParentElement, SharedString, Styled};
use ui::Theme;

mod scale;
pub(crate) use scale::{TextRole, TextTone};

impl TextRole {
    pub(crate) fn color(self, theme: Theme) -> u32 {
        match self.spec().tone {
            TextTone::Primary => theme.text_primary,
            TextTone::Secondary => theme.text_secondary,
            TextTone::Tertiary => theme.text_tertiary,
        }
    }
}

/// Applies a [`TextRole`] to any GPUI styled element.
pub(crate) trait SettingsText: Styled + Sized {
    /// Size, weight, and line height only; use inside components that own
    /// their foreground color (buttons, inputs, radios).
    fn settings_font(self, role: TextRole) -> Self {
        let spec = role.spec();
        self.text_size(px(spec.size))
            .font_weight(FontWeight(spec.weight))
            .line_height(px(spec.line_height))
    }

    /// Full role styling including its default tone. Chain `.text_color(..)`
    /// afterwards only for semantic states.
    fn settings_text(self, role: TextRole, theme: Theme) -> Self {
        self.settings_font(role).text_color(rgba(role.color(theme)))
    }
}

impl<T: Styled> SettingsText for T {}

/// Button label routed through [`TextRole::Control`].
///
/// gpui-component sizes `Button::label` from the button size (16px for the
/// default size), so Settings buttons pass their text as a child instead.
pub(crate) fn control_label(label: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .settings_font(TextRole::Control)
        .child(label.into())
}
