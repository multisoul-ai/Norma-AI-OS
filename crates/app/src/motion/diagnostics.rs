//! Lightweight motion-state counters for opt-in frame diagnostics.

use crate::motion::breathing::BreathingState;
use crate::motion::util::EPSILON;
use crate::motion::MotionStore;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MotionDiagnostics {
    pub nodes: usize,
    pub chase_active_nodes: usize,
    pub chase_visible_nodes: usize,
    pub chase_waiting_nodes: usize,
    pub breathing_active_nodes: usize,
    pub breathing_visible_nodes: usize,
    pub breathing_waiting_nodes: usize,
    pub layout_active_nodes: usize,
    pub dispatch_active_effects: usize,
    pub dispatch_pending_effects: usize,
    pub dispatch_particles: usize,
    pub dispatch_duplicate_skips: u64,
    pub dispatch_capacity_skips: u64,
    pub dispatch_unavailable_skips: u64,
    pub dispatch_cancelled_effects: u64,
    pub dispatch_last_update_elapsed: Duration,
}

impl MotionStore {
    pub(crate) fn diagnostics(&self) -> MotionDiagnostics {
        let mut diagnostics = MotionDiagnostics {
            nodes: self.nodes.len(),
            ..MotionDiagnostics::default()
        };

        for node in self.nodes.values() {
            if node.chase_active {
                diagnostics.chase_active_nodes += 1;
            }
            if node.chase_active && node.chase_fade.value > EPSILON {
                diagnostics.chase_visible_nodes += 1;
            }
            if node.chase_active && node.chase_repaint_elapsed > std::time::Duration::ZERO {
                diagnostics.chase_waiting_nodes += 1;
            }
            if node.breathing_state != BreathingState::Inactive
                || node.breathing_fade.value > EPSILON
            {
                diagnostics.breathing_active_nodes += 1;
            }
            if node.breathing_fade.value > EPSILON
                && node.breathing_display_state != BreathingState::Inactive
            {
                diagnostics.breathing_visible_nodes += 1;
            }
            if node.breathing_repaint_elapsed > std::time::Duration::ZERO {
                diagnostics.breathing_waiting_nodes += 1;
            }
            if node.layout_active {
                diagnostics.layout_active_nodes += 1;
            }
        }

        let dispatch = self.dispatch_receipts.diagnostics();
        diagnostics.dispatch_active_effects = dispatch.active;
        diagnostics.dispatch_pending_effects = dispatch.pending;
        diagnostics.dispatch_particles = dispatch.particles;
        diagnostics.dispatch_duplicate_skips = dispatch.duplicate_skips;
        diagnostics.dispatch_capacity_skips = dispatch.capacity_skips;
        diagnostics.dispatch_unavailable_skips = dispatch.unavailable_skips;
        diagnostics.dispatch_cancelled_effects = dispatch.cancelled_effects;
        diagnostics.dispatch_last_update_elapsed = dispatch.last_update_elapsed;

        diagnostics
    }
}
