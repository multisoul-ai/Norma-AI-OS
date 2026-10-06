//! Shared Settings spacing in logical pixels, independent of GPUI.

/// Page and header insets: compact 16 → 20, regular 28 → 32.
pub(crate) const PAGE_PADDING_COMPACT: f32 = 20.0;
pub(crate) const PAGE_PADDING: f32 = 32.0;
/// Space between sections: 16 → 20.
pub(crate) const SECTION_GAP: f32 = 20.0;
/// Compact section stacks: 8 → 10.
pub(crate) const SECTION_GAP_COMPACT: f32 = 10.0;
/// Space between related rows: 10 → 12.
pub(crate) const ROW_GAP: f32 = 12.0;
/// Large group cards: 20 → 24; small cards: 12 → 16.
pub(crate) const CARD_PADDING: f32 = 24.0;
pub(crate) const CARD_PADDING_COMPACT: f32 = 16.0;
/// Filled rows: 10 → 12; form rows: 6 → 8; compact setup rows: 4 → 6.
pub(crate) const ROW_PADDING_Y: f32 = 12.0;
pub(crate) const FORM_ROW_PADDING_Y: f32 = 8.0;
pub(crate) const SETUP_ROW_PADDING_Y: f32 = 6.0;
