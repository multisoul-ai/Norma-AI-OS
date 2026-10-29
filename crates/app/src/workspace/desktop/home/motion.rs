use std::time::{Duration, Instant};

/// Small interruptible transition; rendering owns scheduling, not the clock.
pub(super) struct HomeTween {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
}

impl Default for HomeTween {
    fn default() -> Self {
        Self {
            from: 0.0,
            target: 0.0,
            started: Instant::now(),
            duration: Duration::ZERO,
        }
    }
}

impl HomeTween {
    pub fn target(&mut self, target: bool, now: Instant, duration: Duration, reduced: bool) {
        if self.target == f32::from(target) && !reduced {
            return;
        }
        self.from = self.value(now);
        self.target = f32::from(target);
        self.started = now;
        self.duration = if reduced { Duration::ZERO } else { duration };
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.target;
        }
        let t = (now.duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32())
            .clamp(0.0, 1.0);
        if t == 0.0 {
            return self.from;
        }
        if t == 1.0 {
            return self.target;
        }
        // Material's cubic-bezier(0.2, 0, 0, 1): invert its x coordinate.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..16 {
            let u = (low + high) * 0.5;
            let x = 0.6 * (1.0 - u).powi(2) * u + u.powi(3);
            if x < t {
                low = u;
            } else {
                high = u;
            }
        }
        let u = (low + high) * 0.5;
        let progress = 3.0 * (1.0 - u) * u * u + u.powi(3);
        self.from + (self.target - self.from) * progress
    }

    pub fn active(&self, now: Instant) -> bool {
        self.from != self.target && now.duration_since(self.started) < self.duration
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_home_motion_tests.rs"]
mod tests;
