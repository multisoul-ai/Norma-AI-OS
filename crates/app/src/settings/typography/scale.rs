//! Pure Settings text roles and panel metrics. No GPUI dependency.

/// Semantic text roles used across Settings pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextRole {
    /// Sidebar "Settings" title and the active page title.
    Title,
    /// One-line page description under the page title.
    Subtitle,
    /// Small group header: navigation groups, pane section labels.
    Eyebrow,
    /// Section, card, and pane headers inside a page.
    Headline,
    /// Title of a list row, toggle row, or option card.
    RowTitle,
    /// Default UI text: values, list content, messages.
    Body,
    /// Long-form text such as memory Markdown.
    Reading,
    /// Descriptions, hints, inline errors.
    Helper,
    /// Field labels and short status text next to controls.
    Label,
    /// Button, navigation item, keycap, and segmented-control text.
    Control,
    /// Metadata: versions, sizes, counts, identifiers.
    Caption,
    /// Compact pills and badges; never the only carrier of key content.
    Badge,
    /// One-time pairing and verification codes.
    Code,
}

/// Default tone for a role, resolved against the active theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextTone {
    Primary,
    Secondary,
    Tertiary,
}

/// Resolved typography values for one role.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextSpec {
    pub(crate) size: f32,
    pub(crate) weight: f32,
    pub(crate) line_height: f32,
    pub(crate) tone: TextTone,
}

impl TextRole {
    #[cfg(test)]
    pub(crate) const ALL: [TextRole; 13] = [
        TextRole::Title,
        TextRole::Subtitle,
        TextRole::Eyebrow,
        TextRole::Headline,
        TextRole::RowTitle,
        TextRole::Body,
        TextRole::Reading,
        TextRole::Helper,
        TextRole::Label,
        TextRole::Control,
        TextRole::Caption,
        TextRole::Badge,
        TextRole::Code,
    ];

    pub(crate) const fn spec(self) -> TextSpec {
        use TextTone::{Primary, Secondary, Tertiary};
        let (size, weight, line_height, tone) = match self {
            TextRole::Title => (18.0, 600.0, 378.0 / 16.0, Primary),
            TextRole::Subtitle => (12.0, 400.0, 180.0 / 11.0, Tertiary),
            TextRole::Eyebrow => (10.0, 600.0, 120.0 / 9.0, Secondary),
            TextRole::Headline => (14.0, 600.0, 238.0 / 13.0, Primary),
            TextRole::RowTitle => (12.0, 600.0, 180.0 / 11.0, Primary),
            TextRole::Body => (12.0, 400.0, 192.0 / 11.0, Primary),
            TextRole::Reading => (13.0, 400.0, 247.0 / 12.0, Primary),
            TextRole::Helper => (11.0, 400.0, 154.0 / 10.0, Secondary),
            TextRole::Label => (11.0, 500.0, 143.0 / 10.0, Secondary),
            TextRole::Control => (12.0, 500.0, 180.0 / 11.0, Secondary),
            TextRole::Caption => (10.0, 500.0, 120.0 / 9.0, Tertiary),
            TextRole::Badge => (9.0, 600.0, 11.0, Secondary),
            TextRole::Code => (26.0, 700.0, 780.0 / 24.0, Primary),
        };
        TextSpec {
            size,
            weight,
            line_height,
            tone,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/settings_typography_tests.rs"]
mod settings_typography_tests;
