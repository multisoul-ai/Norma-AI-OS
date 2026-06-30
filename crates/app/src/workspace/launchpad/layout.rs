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
}

pub(crate) const CONTENT_WIDTH: f32 = 560.0;
pub(crate) const ACTION_CARD_WIDTH: f32 = 180.0;
pub(crate) const ACTION_CARD_HEIGHT: f32 = 92.0;

const SAFE_EDGE: f32 = 24.0;
const WIDE_THRESHOLD: f32 = 1180.0;
const OPTION_ONE_CONTENT_LEFT: f32 = 118.0;
const OPTION_ONE_CONTENT_TOP: f32 = 186.0;
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

            LaunchpadLayout {
                mode,
                content_left,
                content_top,
                content_width,
                content_max_height,
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
            }
        }
    }
}
