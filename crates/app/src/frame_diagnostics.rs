//! Opt-in frame diagnostics for animation and canvas performance analysis.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use canvas::NodeId;

use crate::motion::MotionDiagnostics;

const FRAME_DIAGNOSTICS_ENV: &str = "MULTISOUL_FRAME_DIAGNOSTICS";
const REPAINT_DIAGNOSTICS_LOG_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(test)]
const REASON_RUNTIME_CHANGED: u16 = 1 << 0;
#[cfg(test)]
const REASON_RUNTIME_EVENTS: u16 = 1 << 1;
#[cfg(test)]
const REASON_AGENT_HOOK_CHANGED: u16 = 1 << 2;
#[cfg(test)]
const REASON_MOTION_CHANGED: u16 = 1 << 3;
#[cfg(test)]
const REASON_CURSOR_CHANGED: u16 = 1 << 4;
#[cfg(test)]
const REASON_PERSISTENCE_CHANGED: u16 = 1 << 5;
#[cfg(test)]
const REASON_NORMA_PENDING: u16 = 1 << 6;
#[cfg(test)]
const REASON_AGENT_HOOK_PENDING: u16 = 1 << 7;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameTickReport {
    pub runtime_changed: bool,
    pub runtime_event_count: usize,
    pub agent_hook_changed: bool,
    pub motion_changed: bool,
    pub cursor_changed: bool,
    pub persistence_changed: bool,
    pub norma_pending: bool,
    pub agent_hook_pending: bool,
    pub elapsed: Duration,
}

#[cfg(test)]
impl FrameTickReport {
    pub(crate) fn should_notify(self) -> bool {
        self.runtime_changed
            || self.runtime_event_count > 0
            || self.agent_hook_changed
            || self.motion_changed
            || self.cursor_changed
            || self.persistence_changed
            || self.norma_pending
            || self.agent_hook_pending
    }

    pub(crate) fn elapsed_ms(self) -> f64 {
        self.elapsed.as_secs_f64() * 1000.0
    }

    pub(crate) fn notify_reason_bits(self) -> u16 {
        let mut bits = 0;
        if self.runtime_changed {
            bits |= REASON_RUNTIME_CHANGED;
        }
        if self.runtime_event_count > 0 {
            bits |= REASON_RUNTIME_EVENTS;
        }
        if self.agent_hook_changed {
            bits |= REASON_AGENT_HOOK_CHANGED;
        }
        if self.motion_changed {
            bits |= REASON_MOTION_CHANGED;
        }
        if self.cursor_changed {
            bits |= REASON_CURSOR_CHANGED;
        }
        if self.persistence_changed {
            bits |= REASON_PERSISTENCE_CHANGED;
        }
        if self.norma_pending {
            bits |= REASON_NORMA_PENDING;
        }
        if self.agent_hook_pending {
            bits |= REASON_AGENT_HOOK_PENDING;
        }
        bits
    }

    pub(crate) fn notify_reason_labels(self) -> String {
        let mut labels = Vec::new();
        if self.runtime_changed {
            labels.push("runtime_changed");
        }
        if self.runtime_event_count > 0 {
            labels.push("runtime_events");
        }
        if self.agent_hook_changed {
            labels.push("agent_hook_changed");
        }
        if self.motion_changed {
            labels.push("motion_changed");
        }
        if self.cursor_changed {
            labels.push("cursor_changed");
        }
        if self.persistence_changed {
            labels.push("persistence_changed");
        }
        if self.norma_pending {
            labels.push("norma_pending");
        }
        if self.agent_hook_pending {
            labels.push("agent_hook_pending");
        }
        if labels.is_empty() {
            "none".to_string()
        } else {
            labels.join("|")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasRenderReport {
    pub total_nodes: usize,
    pub visible_nodes: usize,
    pub snake_nodes: usize,
    pub element_count: usize,
    pub zoom: f32,
    pub window_width: f32,
    pub window_height: f32,
    pub elapsed: Duration,
}

impl CanvasRenderReport {
    pub(crate) fn log_if_enabled(self) {
        if !enabled() {
            return;
        }
        tracing::debug!(
            target: "multisoul_frame",
            elapsed_ms = self.elapsed.as_secs_f64() * 1000.0,
            total_nodes = self.total_nodes,
            visible_nodes = self.visible_nodes,
            snake_nodes = self.snake_nodes,
            element_count = self.element_count,
            zoom = self.zoom,
            window_width = self.window_width,
            window_height = self.window_height,
            "canvas_render"
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RepaintDiagnosticsSnapshot {
    pub(crate) requested: u64,
    pub(crate) coalesced: u64,
    pub(crate) emitted: u64,
    pub(crate) last_pending_age: Duration,
    pub(crate) last_selected_budget: Duration,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RepaintDiagnostics {
    enabled: bool,
    snapshot: RepaintDiagnosticsSnapshot,
    last_log_at: Instant,
}

impl RepaintDiagnostics {
    pub(crate) fn new(enabled: bool, now: Instant) -> Self {
        Self {
            enabled,
            snapshot: RepaintDiagnosticsSnapshot::default(),
            last_log_at: now,
        }
    }

    pub(crate) fn record_request(&mut self, coalesced: bool) {
        if !self.enabled {
            return;
        }
        self.snapshot.requested = self.snapshot.requested.saturating_add(1);
        if coalesced {
            self.snapshot.coalesced = self.snapshot.coalesced.saturating_add(1);
        }
    }

    pub(crate) fn record_selected_budget(&mut self, budget: Duration) {
        if self.enabled {
            self.snapshot.last_selected_budget = budget;
        }
    }

    pub(crate) fn record_emission(&mut self, pending_age: Duration, budget: Duration) {
        if !self.enabled {
            return;
        }
        self.snapshot.emitted = self.snapshot.emitted.saturating_add(1);
        self.snapshot.last_pending_age = pending_age;
        self.snapshot.last_selected_budget = budget;
    }

    #[cfg(test)]
    pub(crate) fn snapshot(self) -> RepaintDiagnosticsSnapshot {
        self.snapshot
    }

    pub(crate) fn log_if_due(&mut self, now: Instant) {
        if !self.enabled
            || now.saturating_duration_since(self.last_log_at) < REPAINT_DIAGNOSTICS_LOG_INTERVAL
        {
            return;
        }
        self.last_log_at = now;
        tracing::info!(
            target: "multisoul_frame",
            requested_repaints = self.snapshot.requested,
            coalesced_repaints = self.snapshot.coalesced,
            emitted_repaints = self.snapshot.emitted,
            pending_age_ms = self.snapshot.last_pending_age.as_secs_f64() * 1000.0,
            selected_budget_ms = self.snapshot.last_selected_budget.as_secs_f64() * 1000.0,
            "frame_repaint_summary"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnakePaintReport {
    pub node_id: NodeId,
    pub width_px: f32,
    pub height_px: f32,
    pub zoom: f32,
    pub path_count: usize,
    pub elapsed: Duration,
}

impl SnakePaintReport {
    pub(crate) fn log_if_enabled(self) {
        if !enabled() {
            return;
        }
        tracing::debug!(
            target: "multisoul_frame",
            node_id = self.node_id,
            elapsed_ms = self.elapsed.as_secs_f64() * 1000.0,
            width_px = self.width_px,
            height_px = self.height_px,
            zoom = self.zoom,
            path_count = self.path_count,
            "snake_paths"
        );
    }
}

pub(crate) fn log_motion_sample(diagnostics: MotionDiagnostics) {
    if !enabled() {
        return;
    }
    tracing::debug!(
        target: "multisoul_frame",
        nodes = diagnostics.nodes,
        chase_active_nodes = diagnostics.chase_active_nodes,
        chase_visible_nodes = diagnostics.chase_visible_nodes,
        chase_waiting_nodes = diagnostics.chase_waiting_nodes,
        breathing_active_nodes = diagnostics.breathing_active_nodes,
        breathing_visible_nodes = diagnostics.breathing_visible_nodes,
        breathing_waiting_nodes = diagnostics.breathing_waiting_nodes,
        layout_active_nodes = diagnostics.layout_active_nodes,
        "motion_sample"
    );
}

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let value = std::env::var(FRAME_DIAGNOSTICS_ENV).ok();
        enabled_from_value(value.as_deref())
    })
}

pub(crate) fn enabled_from_value(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../tests/unit/frame_diagnostics_tests.rs"]
mod tests;
