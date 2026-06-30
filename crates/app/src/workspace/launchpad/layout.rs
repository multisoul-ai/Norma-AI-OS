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

pub(crate) const CONTENT_WIDTH: f32 = 560.0;
pub(crate) const ACTION_CARD_WIDTH: f32 = 180.0;
pub(crate) const ACTION_CARD_HEIGHT: f32 = 92.0;
pub(crate) const PREVIEW_WIDTH: f32 = 520.0;
pub(crate) const PREVIEW_HEIGHT: f32 = 610.0;

const SAFE_EDGE: f32 = 24.0;
const WIDE_THRESHOLD: f32 = 1180.0;
const OPTION_ONE_CONTENT_LEFT: f32 = 118.0;
const OPTION_ONE_CONTENT_TOP: f32 = 186.0;
const OPTION_ONE_PREVIEW_RIGHT: f32 = 112.0;
const OPTION_ONE_PREVIEW_TOP: f32 = 142.0;
const MIN_SHORT_TOP: f32 = 84.0;

pub(crate) fn launchpad_layout_for_window(width: f32, height: f32) -> LaunchpadLayout {
    let mode = if width >= WIDE_THRESHOLD {
        LaunchpadLayoutMode::Wide
    } else {
        LaunchpadLayoutMode::Compact
    };
    let content_width = CONTENT_WIDTH.min((width - SAFE_EDGE * 2.0).max(280.0));
    let content_top = ((height - 512.0) * 0.48).clamp(MIN_SHORT_TOP, OPTION_ONE_CONTENT_TOP);
    let content_max_height = (height - content_top - SAFE_EDGE).max(320.0);

    match mode {
        LaunchpadLayoutMode::Wide => {
            let wide_t = ((width - WIDE_THRESHOLD) / (1440.0 - WIDE_THRESHOLD)).clamp(0.0, 1.0);
            let content_left = SAFE_EDGE + (OPTION_ONE_CONTENT_LEFT - SAFE_EDGE) * wide_t;
            let preview_right = if width >= 1440.0 {
                OPTION_ONE_PREVIEW_RIGHT
            } else {
                SAFE_EDGE
            };
            let preview_left = (width - preview_right - PREVIEW_WIDTH).max(
                content_left + content_width + ((width - WIDE_THRESHOLD) * 0.3).clamp(52.0, 96.0),
            );
            let preview_top = OPTION_ONE_PREVIEW_TOP.min((content_top - 44.0).max(80.0));

            LaunchpadLayout {
                mode,
                content_left,
                content_top,
                content_width,
                content_max_height,
                preview_visible: true,
                preview_left,
                preview_top,
                preview_width: PREVIEW_WIDTH,
                preview_height: PREVIEW_HEIGHT,
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
