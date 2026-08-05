use std::time::Duration;

use super::breathing::BreathingState;
use super::util::EPSILON;
use super::NodeMotion;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MotionActivitySummary {
    pub(crate) canvas_active: bool,
    pub(crate) frame_synced_active: bool,
    pub(crate) ambient_active: bool,
    pub(crate) next_visual_boundary: Option<Duration>,
}

impl MotionActivitySummary {
    pub(super) fn from_sources(
        frame_synced_active: bool,
        ambient_active: bool,
        next_visual_boundary: Option<Duration>,
    ) -> Self {
        Self {
            canvas_active: frame_synced_active || ambient_active,
            frame_synced_active,
            ambient_active,
            next_visual_boundary,
        }
    }
}

impl NodeMotion {
    pub(super) fn frame_synced_active(&self) -> bool {
        let scalar_active = [
            self.hover,
            self.pressed,
            self.selected,
            self.focused,
            self.dragging,
            self.opacity,
            self.breathing_fade,
            self.completion_settle,
            self.chase_fade,
        ]
        .into_iter()
        .any(|scalar| (scalar.value - scalar.target).abs() > EPSILON);
        scalar_active || self.layout_active
    }

    pub(super) fn ambient_active(&self, reduce_motion: bool) -> bool {
        !reduce_motion && (self.breathing_state != BreathingState::Inactive || self.chase_active)
    }
}
