#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchpadLayoutMode {
    Wide,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LaunchpadLayout {
    pub mode: LaunchpadLayoutMode,
    pub content_left: f32,
    pub content_top: f32,
    pub content_width: f32,
    pub content_max_height: f32,
    pub preview_visible: bool,
    pub preview_left: f32,
    pub preview_top: f32,
    pub preview_width: f32,
    pub preview_height: f32,
}

pub(crate) const CONTENT_WIDTH: f32 = 500.0;
pub(crate) const ACTION_CARD_WIDTH: f32 = 168.0;
pub(crate) const ACTION_CARD_HEIGHT: f32 = 44.0;

const SAFE_EDGE: f32 = 24.0;
const WIDE_THRESHOLD: f32 = 1180.0;
const REVIEWED_CONTENT_LEFT: f32 = 96.0;
const REVIEWED_CONTENT_TOP: f32 = 92.0;
const PREVIEW_GAP: f32 = 56.0;
const PREVIEW_RIGHT: f32 = 64.0;
const PREVIEW_MAX_HEIGHT: f32 = 720.0;
const MIN_SHORT_TOP: f32 = 84.0;

pub(crate) fn launchpad_layout_for_window(width: f32, height: f32) -> LaunchpadLayout {
    let mode = if width >= WIDE_THRESHOLD {
        LaunchpadLayoutMode::Wide
    } else {
        LaunchpadLayoutMode::Compact
    };
    let content_width = CONTENT_WIDTH.min((width - SAFE_EDGE * 2.0).max(280.0));
    let content_top = (MIN_SHORT_TOP + (height - 640.0).max(0.0) * 0.031)
        .clamp(MIN_SHORT_TOP, REVIEWED_CONTENT_TOP);
    let content_max_height = (height - content_top - SAFE_EDGE).max(320.0);

    match mode {
        LaunchpadLayoutMode::Wide => {
            let wide_t = ((width - WIDE_THRESHOLD) / (1440.0 - WIDE_THRESHOLD)).clamp(0.0, 1.0);
            let content_left = SAFE_EDGE + (REVIEWED_CONTENT_LEFT - SAFE_EDGE) * wide_t;
            let preview_left = content_left + content_width + PREVIEW_GAP;
            let preview_width = (width - preview_left - PREVIEW_RIGHT).max(0.0);
            let preview_height = (height - content_top - 48.0).clamp(0.0, PREVIEW_MAX_HEIGHT);

            LaunchpadLayout {
                mode,
                content_left,
                content_top,
                content_width,
                content_max_height,
                preview_visible: preview_width >= 420.0 && preview_height >= 420.0,
                preview_left,
                preview_top: content_top,
                preview_width,
                preview_height,
            }
        }
        LaunchpadLayoutMode::Compact => {
            let content_left = ((width - content_width) * 0.5).max(SAFE_EDGE);

            LaunchpadLayout {
                mode,
                content_left,
                content_top,
                content_width,
                content_max_height,
                preview_visible: false,
                preview_left: 0.0,
                preview_top: 0.0,
                preview_width: 0.0,
                preview_height: 0.0,
            }
        }
    }
}
