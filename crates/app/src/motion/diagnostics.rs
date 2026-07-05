//! Lightweight motion-state counters for opt-in frame diagnostics.

use crate::motion::breathing::BreathingState;
use crate::motion::util::EPSILON;
use crate::motion::MotionStore;

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

        diagnostics
    }
}
