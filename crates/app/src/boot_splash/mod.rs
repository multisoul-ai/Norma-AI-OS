//! Cold-start boot animation ("Sync"): a full-window video over the first window,
//! dismissed when it ends, on any click or key, or immediately on failure.
//! Policy and timing are pure so they can be tested without a window.

#[cfg(target_os = "macos")]
mod view;
#[cfg(target_os = "macos")]
pub(crate) use view::BootSplash;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Bundled variants (crates/app/assets/boot → Resources/boot); one is picked per cold launch.
const BOOT_VARIANTS: [&str; 2] = ["boot/boot-sync.mp4", "boot/boot-awaken.mp4"];
/// Length of the Sync video; also the default for an unknown asset.
const MAIN_DURATION: Duration = Duration::from_millis(6000);
const AWAKEN_DURATION: Duration = Duration::from_millis(9000);
const FADE_OUT: Duration = Duration::from_millis(450);
const SKIP_FADE: Duration = Duration::from_millis(250);
/// Input this soon after launch is leftover from starting the app (e.g. Spotlight Return), not a skip.
const SKIP_GRACE: Duration = Duration::from_millis(800);
/// Never hold the app behind a black cover if decoding cannot start.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_millis(1500);

static COLD_START_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Only the first window of the process gets the boot animation (not Dock reopen).
pub(crate) fn claim_cold_start() -> bool {
    !COLD_START_CLAIMED.swap(true, Ordering::AcqRel)
}

/// Each bundled video's length; keep in sync with crates/app/assets/boot/README.md.
pub(crate) fn variant_duration(asset: &str) -> Duration {
    if asset == BOOT_VARIANTS[1] {
        AWAKEN_DURATION
    } else {
        MAIN_DURATION
    }
}

pub(crate) fn boot_variant_for(seed: u64) -> &'static str {
    BOOT_VARIANTS[(seed % BOOT_VARIANTS.len() as u64) as usize]
}

/// `NORMA_E2E_BOOT_ANIMATION=sync|awaken` pins a variant for acceptance; any other value means random.
pub(crate) fn forced_boot_variant(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "sync" => Some(BOOT_VARIANTS[0]),
        "awaken" => Some(BOOT_VARIANTS[1]),
        _ => None,
    }
}

/// Reduce motion always wins; e2e-controlled processes only play when a scenario opts in.
pub(crate) fn should_play_boot_splash(
    reduce_motion: bool,
    e2e_controlled: bool,
    e2e_opt_in: bool,
) -> bool {
    !reduce_motion && (!e2e_controlled || e2e_opt_in)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Waiting {
        since: Duration,
    },
    Playing {
        first_frame: Duration,
        length: Duration,
    },
    Fading {
        from: Duration,
        length: Duration,
    },
    Done,
}

/// Times are offsets from when the splash started.
#[derive(Clone, Debug)]
pub(crate) struct BootTimeline {
    phase: Phase,
    length: Duration,
}

impl BootTimeline {
    #[cfg(test)]
    pub(crate) fn new(now: Duration) -> Self {
        Self::with_duration(now, MAIN_DURATION)
    }

    pub(crate) fn with_duration(now: Duration, length: Duration) -> Self {
        Self {
            phase: Phase::Waiting { since: now },
            length,
        }
    }

    pub(crate) fn on_first_frame(&mut self, now: Duration) {
        if matches!(self.phase, Phase::Waiting { .. }) {
            self.phase = Phase::Playing {
                first_frame: now,
                length: self.length,
            };
        }
    }

    pub(crate) fn skip(&mut self, now: Duration) {
        if now < SKIP_GRACE {
            return;
        }
        if matches!(self.phase, Phase::Waiting { .. } | Phase::Playing { .. }) {
            self.phase = Phase::Fading {
                from: now,
                length: SKIP_FADE,
            };
        }
    }

    pub(crate) fn fail(&mut self) {
        self.phase = Phase::Done;
    }

    pub(crate) fn advance(&mut self, now: Duration) {
        self.phase = match self.phase {
            Phase::Waiting { since } if now.saturating_sub(since) >= FIRST_FRAME_TIMEOUT => {
                Phase::Done
            }
            Phase::Playing {
                first_frame,
                length,
            } if now >= first_frame + length - FADE_OUT => Phase::Fading {
                from: first_frame + length - FADE_OUT,
                length: FADE_OUT,
            },
            Phase::Fading { from, length, .. } if now >= from + length => Phase::Done,
            phase => phase,
        };
    }

    pub(crate) fn opacity(&self, now: Duration) -> f32 {
        match self.phase {
            Phase::Waiting { .. } | Phase::Playing { .. } => 1.0,
            Phase::Fading { from, length } => {
                let progress = now.saturating_sub(from).as_secs_f32() / length.as_secs_f32();
                // ease-out so the app is revealed quickly, then settles
                let eased = 1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(2);
                1.0 - eased
            }
            Phase::Done => 0.0,
        }
    }

    pub(crate) fn is_done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/boot_splash_tests.rs"]
mod tests;
