use super::MultisoulApp;
use crate::app::frame_throttle::CURSOR_REPAINT_INTERVAL;
use crate::cards::term::cursor::advance_terminal_cursor_phase as advance_cursor_phase;
use canvas::NodeType;
use runtime::{InputMode, NodeRunState};
use std::time::Duration;

pub(super) fn focused_cursor_animation_active(
    window_active: bool,
    node_type: NodeType,
    node_run_state: Option<NodeRunState>,
) -> bool {
    if !window_active {
        return false;
    }
    match node_type {
        NodeType::Term | NodeType::Agent => {
            matches!(node_run_state, Some(NodeRunState::Running))
        }
        NodeType::Rect | NodeType::Preview | NodeType::Browser => false,
    }
}

impl MultisoulApp {
    pub(super) fn advance_terminal_cursor_phase(&mut self, delta: Duration) -> bool {
        if !self.focused_terminal_cursor_animation_active() {
            self.terminal_cursor_repaint.reset();
            return false;
        }
        let Some(delta) = self
            .terminal_cursor_repaint
            .tick(delta, CURSOR_REPAINT_INTERVAL)
        else {
            return false;
        };
        let next = advance_cursor_phase(self.terminal_cursor_phase, delta);
        let changed = (next - self.terminal_cursor_phase).abs() > f32::EPSILON;
        self.terminal_cursor_phase = next;
        changed
    }

    pub(super) fn focused_terminal_cursor_animation_active(&self) -> bool {
        if self.motion.reduce_motion {
            return false;
        }
        let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() else {
            return false;
        };
        let Some(node) = self.runtime.document.node(node_id) else {
            return false;
        };
        focused_cursor_animation_active(
            self.window_active,
            node.kind.node_type(),
            self.runtime.node_run_state(node_id),
        )
    }
}
