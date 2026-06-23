//! Shared canvas interaction state.

use crate::text_selection::{TextPoint, TextSelection};
use canvas::{NodeId, Vec2};

#[derive(Clone, Copy)]
pub struct DragState {
    pub node: NodeId,
    pub last_screen: Vec2,
}

pub struct InteractionState {
    pub space_held: bool,
    pub panning: bool,
    pub last_screen: Option<Vec2>,
    pub drag: Option<DragState>,
    pub canvas_scroll_enabled: bool,
    pub keyboard_selected_node: Option<NodeId>,
    pub active_text_selection: Option<TextSelection>,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            space_held: false,
            panning: false,
            last_screen: None,
            drag: None,
            canvas_scroll_enabled: false,
            keyboard_selected_node: None,
            active_text_selection: None,
        }
    }
}

impl InteractionState {
    pub fn begin_text_selection(&mut self, node_id: NodeId, anchor: TextPoint) {
        self.active_text_selection = Some(TextSelection::new(node_id, anchor));
    }

    pub fn update_text_selection_head(&mut self, node_id: NodeId, head: TextPoint) -> bool {
        let Some(selection) = self.active_text_selection.as_mut() else {
            return false;
        };
        if selection.node_id != node_id || !selection.dragging || selection.head == head {
            return false;
        }
        selection.head = head;
        true
    }

    pub fn finish_text_selection(&mut self) {
        let Some(selection) = self.active_text_selection else {
            return;
        };
        if selection.is_empty() {
            self.active_text_selection = None;
        } else {
            self.active_text_selection = Some(selection.finish());
        }
    }

    pub fn clear_text_selection(&mut self) {
        self.active_text_selection = None;
    }

    pub fn clear_text_selection_for_node(&mut self, node_id: NodeId) {
        if self
            .active_text_selection
            .map(|selection| selection.node_id == node_id)
            .unwrap_or(false)
        {
            self.active_text_selection = None;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/interaction_tests.rs"]
mod tests;
