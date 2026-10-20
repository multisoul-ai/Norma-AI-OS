//! Reversible side drawer motion. The logical open state remains in runtime.

use crate::motion::{
    scalar::AnimatedScalar,
    util::{ease_out_cubic, lerp},
};
use std::time::Duration;

pub(super) const LEFT_MIN_WIDTH: f32 = 150.0;
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
    pub(crate) left: SideDrawerMotion,
    pub(crate) right: SideDrawerMotion,
}

impl Default for DesktopAssistantDrawerMotion {
    fn default() -> Self {
        Self {
            left: SideDrawerMotion::new(false),
            right: SideDrawerMotion::new(false),
        }
    }
}

impl DesktopAssistantDrawerMotion {
    #[cfg(test)]
    pub(crate) fn settle_left(&mut self, open: bool) {
        self.left = SideDrawerMotion::new(open);
    }

    pub(crate) fn retarget(&mut self, left_open: bool, right_open: bool) {
        self.left.retarget(left_open);
        self.right.retarget(right_open);
    }

    pub(crate) fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        self.left.tick(delta, reduce_motion) | self.right.tick(delta, reduce_motion)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.left.is_active() || self.right.is_active()
    }

    #[cfg(test)]
    pub(crate) fn layout(
        &self,
        window_width: f32,
        left_open: bool,
        right_open: bool,
        reduce_motion: bool,
    ) -> DesktopDrawerLayout {
        self.layout_with_widths(
            window_width,
            left_open,
            right_open,
            reduce_motion,
            None,
            None,
        )
    }

    pub(crate) fn layout_with_widths(
        &self,
        window_width: f32,
        left_open: bool,
        right_open: bool,
        reduce_motion: bool,
        left_width: Option<f32>,
        right_width: Option<f32>,
    ) -> DesktopDrawerLayout {
        let left_fraction = if reduce_motion {
            if left_open {
                1.0
            } else {
                0.0
            }
        } else {
            self.left.fraction(false)
        };
        let right_fraction = if reduce_motion {
            if right_open {
                1.0
            } else {
                0.0
            }
        } else {
            self.right.fraction(false)
        };
        DesktopDrawerLayout::with_widths(
            window_width,
            left_fraction,
            right_fraction,
            left_width,
            right_width,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DesktopDrawerLayout {
    pub(crate) left_width: f32,
    pub(crate) right_width: f32,
    pub(crate) left_space: f32,
    pub(crate) right_space: f32,
    pub(crate) canvas_width: f32,
}

impl DesktopDrawerLayout {
    #[cfg(test)]
    pub(crate) fn new(window_width: f32, left_fraction: f32, right_fraction: f32) -> Self {
        Self::with_widths(window_width, left_fraction, right_fraction, None, None)
    }

    pub(crate) fn with_widths(
        window_width: f32,
        left_fraction: f32,
        right_fraction: f32,
        left_width: Option<f32>,
        right_width: Option<f32>,
    ) -> Self {
        let mut left_width = left_width
            .unwrap_or((window_width * 0.31).clamp(366.0, 438.0))
            .max(LEFT_MIN_WIDTH);
        let mut right_width = right_width
            .unwrap_or((window_width * 0.37).clamp(RIGHT_MIN_WIDTH, 560.0))
            .max(RIGHT_MIN_WIDTH);
        // Match the prototype: only the mobile breakpoint uses overlay drawers.
        let reserve = window_width >= 760.0;
        if reserve && left_fraction > 0.0 && right_fraction > 0.0 {
            let minimum_total = LEFT_MIN_WIDTH + RIGHT_MIN_WIDTH;
            let available_total = (window_width - MIN_CANVAS_WIDTH).max(minimum_total);
            let extra_left = left_width - LEFT_MIN_WIDTH;
            let extra_right = right_width - RIGHT_MIN_WIDTH;
            let extra_total = extra_left + extra_right;
            if extra_total > 0.0 && minimum_total + extra_total > available_total {
                let ratio = (available_total - minimum_total) / extra_total;
                left_width = LEFT_MIN_WIDTH + extra_left * ratio;
                right_width = RIGHT_MIN_WIDTH + extra_right * ratio;
            }
        } else if reserve && left_fraction > 0.0 {
            left_width = left_width.min((window_width - MIN_CANVAS_WIDTH).max(LEFT_MIN_WIDTH));
        } else if reserve && right_fraction > 0.0 {
            right_width = right_width.min((window_width - MIN_CANVAS_WIDTH).max(RIGHT_MIN_WIDTH));
        }
        let left_space = if reserve {
            left_width * left_fraction
        } else {
            0.0
        };
        let right_space = if reserve {
            right_width * right_fraction
        } else {
            0.0
        };
        Self {
            left_width,
            right_width,
            left_space,
            right_space,
            canvas_width: (window_width - left_space - right_space).max(1.0),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_side_drawer_motion_tests.rs"]
mod tests;
