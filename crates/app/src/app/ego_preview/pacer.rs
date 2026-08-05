//! 将不稳定的新内容到达节奏转换为稳定的 UI 呈现节奏。

use std::time::{Duration, Instant};

pub(super) struct EgoFramePacer<T> {
    latest: Option<T>,
    next_present_at: Option<Instant>,
    sequence: u64,
    interval: Duration,
}

impl<T: Clone> EgoFramePacer<T> {
    pub(super) fn new(interval: Duration) -> Self {
        Self {
            latest: None,
            next_present_at: None,
            sequence: 0,
            interval,
        }
    }

    pub(super) fn push(&mut self, frame: T) {
        self.latest = Some(frame);
    }

    pub(super) fn poll(&mut self, now: Instant) -> Option<(u64, T)> {
        let frame = self.latest.clone()?;
        if self
            .next_present_at
            .is_some_and(|next_present_at| now < next_present_at)
        {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1);
        let next_present_at = self
            .next_present_at
            .map_or(now + self.interval, |previous| previous + self.interval);
        self.next_present_at = Some(if next_present_at <= now {
            now + self.interval
        } else {
            next_present_at
        });
        Some((self.sequence, frame))
    }
}
