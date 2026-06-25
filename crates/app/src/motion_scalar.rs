//! Small scalar animation primitive shared by app motion state.

use std::time::Duration;

use crate::motion_util::EPSILON;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnimatedScalar {
    pub(crate) value: f32,
    pub(crate) target: f32,
}

impl AnimatedScalar {
    pub(crate) fn off() -> Self {
        Self {
            value: 0.0,
            target: 0.0,
        }
    }

    pub(crate) fn on() -> Self {
        Self {
            value: 1.0,
            target: 1.0,
        }
    }

    pub(crate) fn set_bool(&mut self, active: bool) {
        self.target = if active { 1.0 } else { 0.0 };
    }

    pub(crate) fn tick(&mut self, delta: Duration, duration: Duration) -> bool {
        if (self.value - self.target).abs() <= EPSILON {
            self.value = self.target;
            return false;
        }

        if duration.is_zero() {
            self.value = self.target;
            return true;
        }

        let step = delta.as_secs_f32() / duration.as_secs_f32();
        if self.target > self.value {
            self.value = (self.value + step).min(self.target);
        } else {
            self.value = (self.value - step).max(self.target);
        }
        true
    }
}
