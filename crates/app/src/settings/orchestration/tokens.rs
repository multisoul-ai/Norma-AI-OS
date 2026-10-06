use norma::orchestration::OrchestrationProvider;
use ui::Theme;

pub(super) const NOTICE_HEIGHT: f32 = 47.0;
pub(super) const PROVIDER_ROW_MIN_HEIGHT: f32 = 85.0;
pub(super) const PROVIDER_ROW_GAP: f32 = 6.0;
pub(super) const PROVIDER_TEXT_GAP: f32 = 0.0;
pub(super) const PROVIDER_STATUS_WIDTH: f32 = 259.0;
pub(super) const PROVIDER_ACTION_WIDTH: f32 = 112.0;
pub(super) const PROVIDER_SECTION_RIGHT_INSET: f32 = 7.0;
pub(super) const PROVIDER_ROW_RIGHT_OUTDENT: f32 = 23.0;
pub(super) const COMPONENT_ROW_HEIGHT: f32 = 44.0;
pub(super) const COMPONENT_LABEL_WIDTH: f32 = 152.0;
pub(super) const COMPONENT_DETAILS_BOTTOM_MARGIN: f32 = 14.0;

pub(super) fn provider_row_min_height(provider: OrchestrationProvider, expanded: bool) -> f32 {
    match (provider, expanded) {
        (OrchestrationProvider::Codex, _) => 92.0,
        (OrchestrationProvider::Claude, _) => 84.0,
        (OrchestrationProvider::Cursor, true) => 80.0,
        _ => PROVIDER_ROW_MIN_HEIGHT,
    }
}

pub(super) fn warning_status_color() -> u32 {
    Theme::glass_dark().accent_orange
}

#[cfg(test)]
pub(super) fn canonical_expanded_content_height() -> f32 {
    const CONTENT_HEADER_HEIGHT: f32 = 119.0;
    const COMPONENT_COUNT: f32 = 4.0;
    CONTENT_HEADER_HEIGHT
        + NOTICE_HEIGHT
        + OrchestrationProvider::ALL
            .into_iter()
            .map(|provider| {
                provider_row_min_height(provider, provider == OrchestrationProvider::Cursor)
            })
            .sum::<f32>()
        + COMPONENT_COUNT * COMPONENT_ROW_HEIGHT
        + COMPONENT_DETAILS_BOTTOM_MARGIN
}

#[cfg(test)]
pub(super) fn canonical_provider_typography() -> (f32, f32, f32, f32, f32) {
    (
        crate::settings::typography::TextRole::RowTitle.spec().size,
        crate::settings::typography::TextRole::Helper.spec().size,
        crate::settings::typography::TextRole::Label.spec().size,
        crate::settings::typography::TextRole::Helper.spec().size,
        crate::settings::typography::TextRole::Helper.spec().size,
    )
}

#[cfg(test)]
pub(super) fn canonical_provider_row_layout() -> (f32, f32, f32, f32, f32, f32, f32) {
    (
        PROVIDER_ROW_GAP,
        PROVIDER_TEXT_GAP,
        PROVIDER_STATUS_WIDTH,
        PROVIDER_ACTION_WIDTH,
        PROVIDER_SECTION_RIGHT_INSET,
        PROVIDER_ROW_RIGHT_OUTDENT,
        COMPONENT_LABEL_WIDTH,
    )
}

#[cfg(test)]
pub(super) fn canonical_provider_row_rhythm() -> (f32, f32, f32, f32, f32) {
    (
        provider_row_min_height(OrchestrationProvider::Codex, false),
        provider_row_min_height(OrchestrationProvider::Claude, false),
        provider_row_min_height(OrchestrationProvider::Cursor, true),
        provider_row_min_height(OrchestrationProvider::OpenCode, false),
        provider_row_min_height(OrchestrationProvider::Grok, false),
    )
}
