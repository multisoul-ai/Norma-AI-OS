use super::agent_interrupt::end_active_agent_task_for_manual_interrupt;
use crate::app::{AppScreen, MultisoulApp};
use canvas::CanvasNodeKind;
use gpui::{actions, App, Context, Global, KeyBinding, Window};
use runtime::{route, InputEvent, InputMode, NodeInput, Route};
use terminal::{Key, KeyInput};

actions!(multisoul_tab, [SendTabToFocusedPty]);

struct MultisoulKeyBindingsInitialized;

impl Global for MultisoulKeyBindingsInitialized {}

pub(crate) fn init_multisoul_key_bindings(cx: &mut App) {
    if cx.has_global::<MultisoulKeyBindingsInitialized>() {
        return;
    }
    cx.bind_keys([KeyBinding::new(
        "tab",
        SendTabToFocusedPty,
        Some("MultisoulApp"),
    )]);
    cx.set_global(MultisoulKeyBindingsInitialized);
}

impl MultisoulApp {
    pub(crate) fn on_send_tab_to_focused_pty(
        &mut self,
        _: &SendTabToFocusedPty,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_panel.open || !matches!(self.screen, AppScreen::Canvas) {
            cx.propagate();
            return;
        }

        let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() else {
            cx.propagate();
            return;
        };
        let is_pty_card = self.runtime.document.node(node_id).is_some_and(|node| {
            matches!(
                node.kind,
                CanvasNodeKind::Term(_) | CanvasNodeKind::Agent(_)
            )
        });
        if !is_pty_card
            || route(&self.runtime.focus.mode(), &InputEvent::Key)
                != Route::SendInputToNode(node_id)
        {
            cx.propagate();
            return;
        }

        let input = KeyInput {
            key: Key::Tab,
            ctrl: false,
            alt: false,
        };
        end_active_agent_task_for_manual_interrupt(self, node_id, &input);
        self.interaction.clear_text_selection_for_node(node_id);
        self.motion.interrupt_initialization(node_id);
        if let Err(error) = self.runtime.send_node_input(node_id, NodeInput::Key(input)) {
            tracing::error!(%error, "send Tab to focused PTY node failed");
        }
        cx.notify();
    }
}
