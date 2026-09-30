//! Browser 工具栏下方的加载进度线：不确定进度，随导航开始/完成出现与消失。

use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, Animation, AnimationExt as _, AnyElement, IntoElement};
use std::time::Duration;
use ui::{icons::with_alpha, Theme};

const LOAD_BAR_H_PX: f32 = 2.0;
const LOAD_BAR_CYCLE: Duration = Duration::from_millis(1100);
const LOAD_BAR_SEGMENT: f32 = 0.32;

/// 进度线滑块的左边缘位置（占轨道宽度比例）。
///
/// progress 0→1 时滑块从完全移出左侧（-SEGMENT）滑到完全移出右侧（1.0），
/// 这样循环衔接处滑块不可见，不会出现跳变。
pub(crate) fn load_bar_segment_left(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    -LOAD_BAR_SEGMENT + p * (1.0 + LOAD_BAR_SEGMENT)
}

pub(crate) fn browser_load_bar(
    node_id: NodeId,
    loading: bool,
    reduce_motion: bool,
    theme: Theme,
    zoom: f32,
) -> AnyElement {
    let height = px((LOAD_BAR_H_PX * zoom).max(1.0));
    let track = div()
        .id(("browser-load-bar", node_id))
        .h(height)
        .w_full()
        .flex_none()
        .relative()
        .overflow_hidden();
    if !loading {
        return track.into_any_element();
    }
    let accent = rgba(with_alpha(theme.accent_cyan, 0xd0));
    if reduce_motion {
        return track
            .child(
                div()
                    .size_full()
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x88))),
            )
            .into_any_element();
    }
    track
        .child(
            div()
                .id(("browser-load-bar-segment", node_id))
                .absolute()
                .top_0()
                .h_full()
                .w(gpui::relative(LOAD_BAR_SEGMENT))
                .bg(accent)
                .with_animation(
                    ("browser-load-bar-cycle", node_id),
                    Animation::new(LOAD_BAR_CYCLE).repeat(),
                    |segment, progress| {
                        segment.left(gpui::relative(load_bar_segment_left(progress)))
                    },
                ),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/unit/browser_load_bar_tests.rs"]
mod tests;
