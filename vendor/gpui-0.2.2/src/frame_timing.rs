//! Optional CPU, GPU and presentation timing; never substitutes callback time for presentation.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

// 10s warm-up + 3 × 60s capture + 110s navigation/setup allowance = 300s.
const CAPTURE_DURATION: Duration = Duration::from_secs(300);

pub(crate) fn start() -> Option<Instant> {
    static STARTED: OnceLock<Option<Instant>> = OnceLock::new();
    let started = STARTED.get_or_init(|| {
        (std::env::var("GPUI_FRAME_TIMINGS").as_deref() == Ok("1")).then(Instant::now)
    });
    started
        .filter(|start| start.elapsed() < CAPTURE_DURATION)
        .map(|_| Instant::now())
}

pub(crate) fn elapsed_ms(start: f64, end: f64) -> Option<f64> {
    (start.is_finite() && end.is_finite() && start > 0.0 && end >= start)
        .then_some((end - start) * 1000.0)
}

#[cfg(test)]
#[path = "../tests/unit/frame_timing_tests.rs"]
mod tests;
