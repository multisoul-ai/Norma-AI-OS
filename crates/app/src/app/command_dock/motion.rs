use std::time::Duration;

use crate::motion::{
    scalar::AnimatedScalar,
    util::{ease_out_cubic, lerp},
};

/// Retain the last visible content until the drawer has finished closing.
pub(crate) struct CommandDrawer {
    pub(super) draft: String,
    pub(super) error: Option<ui::i18n::LocalizedText>,
    pub(super) reply: Option<ui::i18n::LocalizedText>,
    pub(super) content_height: f32,
    from_height: f32,
    target_height: f32,
    from_opacity: f32,
    progress: AnimatedScalar,
}

impl Default for CommandDrawer {
    fn default() -> Self {
        Self {
            draft: String::new(),
            error: None,
            reply: None,
            content_height: 0.0,
            from_height: 0.0,
            target_height: 0.0,
            from_opacity: 0.0,
            progress: AnimatedScalar::on(),
        }
    }
}

impl CommandDrawer {
    pub(super) fn retarget(&mut self, height: f32) {
        if self.target_height == height {
            return;
        }
        // Reverse or resize from the displayed frame, never from a keyframe endpoint.
        self.from_height = self.height(false);
        self.from_opacity = self.opacity();
        self.target_height = height;
        self.progress.value = 0.0;
        if height > 0.0 {
            self.content_height = height;
        }
    }

    pub(super) fn height(&self, reduce_motion: bool) -> f32 {
        if reduce_motion {
            self.content_height
        } else {
            lerp(
                self.from_height,
                self.target_height,
                ease_out_cubic(self.progress.value),
            )
        }
    }

    pub(super) fn opacity(&self) -> f32 {
        lerp(
            self.from_opacity,
            if self.target_height > 0.0 { 1.0 } else { 0.0 },
            ease_out_cubic(self.progress.value),
        )
    }

    /// Dock top corners flatten as the drawer opens so the two surfaces read as one.
    pub(crate) fn dock_top_radius(&self, base: f32) -> f32 {
        base * (1.0 - self.opacity())
    }

    pub(crate) fn is_active(&self) -> bool {
        self.progress.value < 1.0
    }

    pub(crate) fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        // Shorter dismissal keeps repeated typing responsive; reduced motion only fades.
        let duration = Duration::from_millis(if reduce_motion || self.target_height == 0.0 {
            160
        } else {
            220
        });
        self.progress.tick(delta, duration)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/command_drawer_tests.rs"]
mod tests;
