use std::time::Duration;

use crate::motion::AMBIENT_REPAINT_INTERVAL;

pub(crate) const CURSOR_REPAINT_INTERVAL: Duration = AMBIENT_REPAINT_INTERVAL;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FrameThrottle {
    elapsed: Duration,
}

impl FrameThrottle {
    pub(crate) fn tick(&mut self, delta: Duration, interval: Duration) -> Option<Duration> {
        self.elapsed += delta;
        if self.elapsed < interval {
            return None;
        }

        let elapsed = self.elapsed;
        self.elapsed = Duration::ZERO;
        Some(elapsed)
    }

    pub(crate) fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
    }
}
