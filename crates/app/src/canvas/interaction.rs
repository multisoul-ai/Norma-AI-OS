//! Shared canvas interaction state.

use crate::canvas::terminal_link::TerminalLinkIntent;
use crate::canvas::text_selection::{TextPoint, TextSelection};
use canvas::{NodeId, Vec2};
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct DragState {
    pub node: NodeId,
    pub last_screen: Vec2,
    pub swap_candidate: Option<NodeId>,
}

#[derive(Clone)]
pub(crate) struct PendingTerminalLink {
    pub(crate) node_id: NodeId,
    pub(crate) anchor: TextPoint,
    pub(crate) pressed_screen: Vec2,
    pub(crate) intent: TerminalLinkIntent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HoveredTerminalLink {
    pub(crate) node_id: NodeId,
    pub(crate) url: String,
    pub(crate) screen_x: i32,
    pub(crate) screen_y: i32,
}

#[derive(Default)]
pub struct InteractionState {
    pub space_held: bool,
    pub panning: bool,
    pub last_screen: Option<Vec2>,
    pub drag: Option<DragState>,
    pub canvas_scroll_enabled: bool,
    pub keyboard_selected_node: Option<NodeId>,
    pub active_text_selection: Option<TextSelection>,
    pub(crate) pending_terminal_link: Option<PendingTerminalLink>,
    pub(crate) hovered_terminal_link: Option<HoveredTerminalLink>,
    pub last_motion_notify: Option<Instant>,
    /// 顶部工具栏 ✚ 按钮的弹出菜单（Terminal/Claude/Codex/Preview）是否展开。
    pub new_menu_open: bool,
}

impl InteractionState {
    /// 切换顶部 ✚ 弹出菜单的展开状态。
    pub fn toggle_new_menu(&mut self) {
        self.new_menu_open = !self.new_menu_open;
    }

    /// 关闭顶部 ✚ 弹出菜单（Esc / 画布点击 / 选中菜单项后调用）。
    pub fn close_new_menu(&mut self) {
        self.new_menu_open = false;
    }

    pub fn begin_text_selection(&mut self, node_id: NodeId, anchor: TextPoint) {
        self.pending_terminal_link = None;
        self.active_text_selection = Some(TextSelection::new(node_id, anchor));
    }

    pub(crate) fn begin_terminal_link(
        &mut self,
        node_id: NodeId,
        anchor: TextPoint,
        pressed_screen: Vec2,
        intent: TerminalLinkIntent,
    ) {
        self.active_text_selection = None;
        self.pending_terminal_link = Some(PendingTerminalLink {
            node_id,
            anchor,
            pressed_screen,
            intent,
        });
    }

    pub(crate) fn take_pending_terminal_link(&mut self) -> Option<PendingTerminalLink> {
        self.pending_terminal_link.take()
    }

    pub(crate) fn update_hovered_terminal_link(
        &mut self,
        next: Option<HoveredTerminalLink>,
    ) -> bool {
        if self
            .hovered_terminal_link
            .as_ref()
            .zip(next.as_ref())
            .map(|(current, next)| current.node_id == next.node_id && current.url == next.url)
            .unwrap_or(self.hovered_terminal_link.is_none() && next.is_none())
        {
            return false;
        }
        self.hovered_terminal_link = next;
        true
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
#[path = "../../tests/unit/interaction_tests.rs"]
mod tests;
