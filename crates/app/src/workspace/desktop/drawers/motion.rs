//! Reversible side drawer motion. The logical open state remains in runtime.

use crate::motion::{
    scalar::AnimatedScalar,
    util::{ease_out_cubic, lerp},
};
use std::time::Duration;

pub(super) const RIGHT_MIN_WIDTH: f32 = 185.0;
pub(super) const MIN_CANVAS_WIDTH: f32 = 320.0;

pub(crate) struct SideDrawerMotion {
    from: f32,
    target: f32,
    from_opacity: f32,
    progress: AnimatedScalar,
}

impl SideDrawerMotion {
    fn new(open: bool) -> Self {
        let value = if open { 1.0 } else { 0.0 };
        Self {
            from: value,
            target: value,
            from_opacity: value,
            progress: AnimatedScalar::on(),
        }
    }

    fn retarget(&mut self, open: bool) {
        let target = if open { 1.0 } else { 0.0 };
        if self.target == target {
            return;
        }
        // Every reversal begins at the presentation value of the prior frame.
        self.from = self.fraction(false);
        self.from_opacity = self.opacity(true);
        self.target = target;
        self.progress.value = 0.0;
    }

    pub(crate) fn fraction(&self, reduce_motion: bool) -> f32 {
        if reduce_motion {
            // Fade in place when Reduce Motion is enabled.
            return if self.should_render() { 1.0 } else { 0.0 };
        }
        lerp(self.from, self.target, ease_out_cubic(self.progress.value))
    }

    pub(crate) fn edge_inset(&self, width: f32, gutter: f32, reduce_motion: bool) -> f32 {
        -width - 2.0 + (width + 2.0 + gutter) * self.fraction(reduce_motion)
    }

    pub(crate) fn opacity(&self, reduce_motion: bool) -> f32 {
        if reduce_motion {
            lerp(
                self.from_opacity,
                self.target,
                ease_out_cubic(self.progress.value),
            )
        } else if self.should_render() {
            1.0
        } else {
            0.0
        }
    }

    pub(crate) fn should_render(&self) -> bool {
        self.target > 0.0 || (self.from > 0.0 && self.is_active())
    }

    fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        let duration = Duration::from_millis(if reduce_motion || self.target == 0.0 {
            160
        } else {
            220
        });
        self.progress.tick(delta, duration)
    }

    fn is_active(&self) -> bool {
        self.progress.value < 1.0
    }
}

pub(crate) struct DesktopAssistantDrawerMotion {
    pub(crate) right: SideDrawerMotion,
}

impl Default for DesktopAssistantDrawerMotion {
    fn default() -> Self {
        Self {
            right: SideDrawerMotion::new(false),
        }
    }
}

impl DesktopAssistantDrawerMotion {
    pub(crate) fn retarget(&mut self, right_open: bool) {
        self.right.retarget(right_open);
    }

    pub(crate) fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        self.right.tick(delta, reduce_motion)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.right.is_active()
    }

    #[cfg(test)]
    pub(crate) fn layout(
        &self,
        window_width: f32,
        right_open: bool,
        reduce_motion: bool,
    ) -> DesktopDrawerLayout {
        self.layout_with_width(window_width, right_open, reduce_motion, None)
    }

    pub(crate) fn layout_with_width(
        &self,
        window_width: f32,
        right_open: bool,
        reduce_motion: bool,
        right_width: Option<f32>,
    ) -> DesktopDrawerLayout {
        let right_fraction = if reduce_motion {
            f32::from(right_open as u8)
        } else {
            self.right.fraction(false)
        };
        DesktopDrawerLayout::with_width(window_width, right_fraction, right_width)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DesktopDrawerLayout {
    pub(crate) right_width: f32,
    pub(crate) right_space: f32,
    pub(crate) canvas_width: f32,
}

impl DesktopDrawerLayout {
    #[cfg(test)]
    pub(crate) fn new(window_width: f32, right_fraction: f32) -> Self {
        Self::with_width(window_width, right_fraction, None)
    }

    pub(crate) fn with_width(
        window_width: f32,
        right_fraction: f32,
        right_width: Option<f32>,
    ) -> Self {
        let mut right_width = right_width
            .unwrap_or((window_width * 0.37).clamp(RIGHT_MIN_WIDTH, 560.0))
            .max(RIGHT_MIN_WIDTH);
        // Match the prototype: only the mobile breakpoint uses an overlay drawer.
        let reserve = window_width >= 760.0;
        if reserve && right_fraction > 0.0 {
            right_width = right_width.min((window_width - MIN_CANVAS_WIDTH).max(RIGHT_MIN_WIDTH));
        }
        let right_space = if reserve {
            right_width * right_fraction
        } else {
            0.0
        };
        Self {
            right_width,
            right_space,
            canvas_width: (window_width - right_space).max(1.0),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_side_drawer_motion_tests.rs"]
mod tests;
