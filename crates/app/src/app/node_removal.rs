use crate::app::MultisoulApp;
use crate::canvas::master_layout::close_node_and_update_layout_for_area;
use canvas::{CanvasNodeKind, NodeId};
use gpui::Window;

impl MultisoulApp {
    pub(crate) fn remove_node_and_reflow(&mut self, node_id: NodeId, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let removed_agent_definition_id = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        });
        if self.interaction.keyboard_selected_node == Some(node_id) {
            self.interaction.keyboard_selected_node = None;
        }
        self.interaction.clear_text_selection_for_node(node_id);

        let removed_node_was_browser = self
            .runtime
            .document
            .node(node_id)
            .map(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .unwrap_or(false);
        let next_id = canvas::next_neighbor_after_delete(self.runtime.document.nodes(), node_id);

        let changed = close_node_and_update_layout_for_area(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            node_id,
            area,
            zoom,
        );
        if changed {
            self.previews.remove(node_id);
            if removed_node_was_browser {
                self.release_browser_input(node_id);
                self.focus_handle.focus(window);
            }
            self.remove_browser_view(node_id);
            if removed_agent_definition_id.is_some() {
                self.clear_conversation_tail_for_node(node_id);
            }
            self.mark_canvas_dirty();
            self.mark_session_dirty();
            if let Some(definition_id) = removed_agent_definition_id {
                if let Err(message) = self
                    .agent_definitions
                    .cleanup_quick_definition(definition_id)
                {
                    self.workspace_state.error = Some(message);
                }
            }

            match next_id {
                Some(next) if self.runtime.document.node(next).is_some() => {
                    self.runtime.selection.select_only(next);
                    self.runtime.focus.exit_to_canvas();
                    self.interaction.keyboard_selected_node = Some(next);
                }
                _ => {
                    self.interaction.keyboard_selected_node = None;
                }
            }
        }
        changed
    }
}
