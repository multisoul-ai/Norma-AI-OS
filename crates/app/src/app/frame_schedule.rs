use std::time::{Duration, Instant};

const VOICE_ANIMATION_INTERVAL: Duration = Duration::from_millis(12);
const ACTIVE_CANVAS_INTERVAL: Duration = Duration::from_millis(16);
const IDLE_ANIMATION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AnimationActivity {
    pub voice_active: bool,
    pub canvas_motion_active: bool,
    pub cursor_active: bool,
    pub screen_is_canvas: bool,
    pub window_active: bool,
    pub settings_open: bool,
    pub next_visual_boundary: Option<Duration>,
}

pub(super) struct AnimationSchedule;

impl AnimationSchedule {
    pub(super) fn next_interval(activity: AnimationActivity) -> Duration {
        if activity.voice_active {
            return VOICE_ANIMATION_INTERVAL;
        }
        let canvas_visible =
            activity.screen_is_canvas && activity.window_active && !activity.settings_open;
        let canvas_active = activity.canvas_motion_active
            || activity.cursor_active
            || activity.next_visual_boundary.is_some();
        if !canvas_visible || !canvas_active {
            return IDLE_ANIMATION_INTERVAL;
        }
        activity
            .next_visual_boundary
            .filter(|duration| !duration.is_zero())
            .map_or(ACTIVE_CANVAS_INTERVAL, |boundary| {
                ACTIVE_CANVAS_INTERVAL.min(boundary)
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum RepaintReason {
    RuntimeChanged = 1 << 0,
    RuntimeEvents = 1 << 1,
    AgentHook = 1 << 2,
    #[cfg(test)]
    TerminalOutput = 1 << 3,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RepaintReasons(u8);

impl RepaintReasons {
    fn insert(&mut self, reason: RepaintReason) {
        self.0 |= reason as u8;
    }

    #[cfg(test)]
    pub(super) fn contains(self, reason: RepaintReason) -> bool {
        self.0 & reason as u8 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RepaintEmission {
    pub(super) reasons: RepaintReasons,
    pub(super) pending_age: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RepaintGate {
    pending_since: Option<Instant>,
    reasons: RepaintReasons,
}

impl RepaintGate {
    pub(super) fn request(&mut self, now: Instant, reason: RepaintReason) -> bool {
        let coalesced = self.pending_since.is_some();
        self.pending_since.get_or_insert(now);
        self.reasons.insert(reason);
        coalesced
    }

    pub(super) fn poll(&mut self, now: Instant, budget: Duration) -> Option<RepaintEmission> {
        let pending_since = self.pending_since?;
        let pending_age = now.saturating_duration_since(pending_since);
        if pending_age < budget {
            return None;
        }
        let emission = RepaintEmission {
            reasons: self.reasons,
            pending_age,
        };
        self.reset();
        Some(emission)
    }

    pub(super) fn reset(&mut self) {
        self.pending_since = None;
        self.reasons = RepaintReasons::default();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RepaintContext {
    pub(super) screen_is_canvas: bool,
    pub(super) settings_open: bool,
    pub(super) window_active: bool,
    pub(super) focused_running_terminal: bool,
    pub(super) voice_active: bool,
}

impl RepaintContext {
    pub(super) fn repaint_budget(self) -> Option<Duration> {
        if !self.screen_is_canvas || self.settings_open {
            return None;
        }
        if self.window_active && (self.focused_running_terminal || self.voice_active) {
            return Some(Duration::from_millis(16));
        }
        Some(Duration::from_millis(33))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/frame_schedule_tests.rs"]
mod tests;
