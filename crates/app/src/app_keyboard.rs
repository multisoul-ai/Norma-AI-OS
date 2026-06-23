//! Keyboard routing for the root GPUI view.

use crate::app::{AppScreen, CopyCardContent, MultisoulApp};
use crate::cards::term::selection::terminal_selected_text;
use crate::cards::term::view::keystroke_to_input;
use crate::text_selection::TextSelection;
use crate::voice_control::{
    is_voice_hold_modifier_release, is_voice_hold_release, is_voice_hold_shortcut, VoiceTrigger,
};
use canvas::{navigate_card, NavigationDirection, NodeId};
use gpui::{
    ClipboardItem, Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ModifiersChangedEvent,
    Window,
};
use runtime::{route, InputEvent, InputMode, NodeInput, NodeView, Route, Runtime};
#[cfg(test)]
use terminal::{Cell, GridSnapshot};

#[cfg(test)]
pub(crate) fn current_copy_node_id(runtime: &Runtime) -> Option<NodeId> {
    if let InputMode::NodeFocused(node_id) = runtime.focus.mode() {
        if runtime.document.node(node_id).is_some() {
            return Some(node_id);
        }
    }

    current_selection_node_id(runtime)
}

fn current_selection_node_id(runtime: &Runtime) -> Option<NodeId> {
    let mut selected = runtime.selection.iter();
    let node_id = selected.next()?;
    if selected.next().is_none() && runtime.document.node(node_id).is_some() {
        Some(node_id)
    } else {
        None
    }
}

#[cfg(test)]
pub(crate) fn terminal_snapshot_copy_text(snapshot: &GridSnapshot) -> String {
    let mut lines = snapshot
        .rows
        .iter()
        .map(|row| terminal_row_copy_text(row))
        .collect::<Vec<_>>();
    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

pub(crate) fn copy_text_for_active_selection(
    runtime: &Runtime,
    active_selection: Option<TextSelection>,
) -> Option<String> {
    let selection = active_selection?;
    let NodeView::Term { snapshot, .. } = runtime.node_view(selection.node_id)?;
    let text = terminal_selected_text(&snapshot, selection.range());
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
fn terminal_row_copy_text(row: &[Cell]) -> String {
    let mut line = row
        .iter()
        .map(|cell| {
            if cell.attrs.hidden || cell.attrs.wide_char_spacer || cell.c == '\0' {
                ' '
            } else {
                cell.c
            }
        })
        .collect::<String>();
    while line.ends_with(' ') {
        line.pop();
    }
    line
}

fn canvas_navigation_direction(key: &str, modifiers: Modifiers) -> Option<NavigationDirection> {
    if modifiers.modified() {
        return None;
    }

    match key {
        "up" => Some(NavigationDirection::Up),
        "down" => Some(NavigationDirection::Down),
        "left" => Some(NavigationDirection::Left),
        "right" => Some(NavigationDirection::Right),
        _ => None,
    }
}

fn keyboard_selected_focus_target(
    runtime: &Runtime,
    keyboard_selected_node: Option<NodeId>,
) -> Option<NodeId> {
    let selected_id = current_selection_node_id(runtime)?;
    if keyboard_selected_node != Some(selected_id) || !runtime.node_caps(selected_id).focusable {
        return None;
    }
    Some(selected_id)
}

fn should_auto_focus_keyboard_selection(keystroke: &Keystroke) -> bool {
    if canvas_navigation_direction(&keystroke.key, keystroke.modifiers).is_some() {
        return false;
    }
    if keystroke.key == "space" && !keystroke.modifiers.modified() {
        return false;
    }
    if keystroke.modifiers.platform {
        return keystroke.key == "v"
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
            && !keystroke.modifiers.function;
    }
    if keystroke.modifiers.control || keystroke.modifiers.function {
        return false;
    }
    if keystroke.key == "escape" {
        return false;
    }

    crate::app_ime::should_defer_keystroke_to_ime(keystroke)
        || keystroke_to_input(keystroke).is_some()
}

impl MultisoulApp {
    pub(crate) fn on_copy_card_content(
        &mut self,
        _: &CopyCardContent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        if let Some(text) =
            copy_text_for_active_selection(&self.runtime, self.interaction.active_text_selection)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// 键盘事件：维护 space_held；聚焦终端时把按键编码写入对应 PTY。
    pub(crate) fn on_key_down(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_voice_hold_shortcut(&ev.keystroke) {
            self.begin_voice_recording(VoiceTrigger::Shortcut, cx);
            return;
        }

        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        let is_space = ev.keystroke.key == "space";
        if is_space && matches!(self.runtime.focus.mode(), InputMode::CanvasSelect) {
            self.interaction.space_held = true;
        }

        if matches!(self.runtime.focus.mode(), InputMode::CanvasSelect) {
            if let Some(direction) =
                canvas_navigation_direction(&ev.keystroke.key, ev.keystroke.modifiers)
            {
                let selected_id = current_selection_node_id(&self.runtime);
                let visible_area = self.visible_canvas_rect(window);
                let target = navigate_card(
                    self.runtime.document.nodes(),
                    selected_id,
                    visible_area,
                    direction,
                );
                let event = InputEvent::NavigateSelection { target };
                if let Route::SelectNode(node_id) = route(&self.runtime.focus.mode(), &event) {
                    self.runtime.selection.select_only(node_id);
                    self.runtime.focus.exit_to_canvas();
                    self.interaction.keyboard_selected_node = Some(node_id);
                    cx.stop_propagation();
                    cx.notify();
                }
                return;
            }

            if let Some(node_id) = keyboard_selected_focus_target(
                &self.runtime,
                self.interaction.keyboard_selected_node,
            ) {
                if should_auto_focus_keyboard_selection(&ev.keystroke) {
                    self.runtime.focus.focus_node(node_id);
                    self.interaction.keyboard_selected_node = None;
                    if ev.keystroke.modifiers.platform && ev.keystroke.key == "v" {
                        if let Some(item) = cx.read_from_clipboard() {
                            if let Some(text) = item.text() {
                                self.interaction.clear_text_selection_for_node(node_id);
                                if let Err(e) =
                                    self.runtime.send_node_input(node_id, NodeInput::Text(text))
                                {
                                    tracing::error!(error = %e, "paste send_node_input failed");
                                }
                            }
                        }
                        cx.notify();
                        return;
                    }

                    if crate::app_ime::should_defer_keystroke_to_ime(&ev.keystroke) {
                        cx.notify();
                        return;
                    }
                    if let Some(input) = keystroke_to_input(&ev.keystroke) {
                        self.interaction.clear_text_selection_for_node(node_id);
                        if let Err(e) = self.runtime.send_node_input(node_id, NodeInput::Key(input))
                        {
                            tracing::error!(error = %e, "send_node_input failed");
                        }
                    }
                    cx.notify();
                    return;
                }
            }
        }

        if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
            if route(&self.runtime.focus.mode(), &InputEvent::Key)
                == Route::SendInputToNode(node_id)
            {
                if ev.keystroke.modifiers.platform && ev.keystroke.key == "v" {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            self.interaction.clear_text_selection_for_node(node_id);
                            if let Err(e) =
                                self.runtime.send_node_input(node_id, NodeInput::Text(text))
                            {
                                tracing::error!(error = %e, "paste send_node_input failed");
                            }
                            cx.notify();
                        }
                    }
                    return;
                }

                if crate::app_ime::should_defer_keystroke_to_ime(&ev.keystroke) {
                    return;
                }
                if let Some(input) = keystroke_to_input(&ev.keystroke) {
                    self.interaction.clear_text_selection_for_node(node_id);
                    if let Err(e) = self.runtime.send_node_input(node_id, NodeInput::Key(input)) {
                        tracing::error!(error = %e, "send_node_input failed");
                    }
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn on_key_up(
        &mut self,
        ev: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_voice_recording_shortcut() && is_voice_hold_release(&ev.keystroke) {
            self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
            return;
        }

        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }

        if ev.keystroke.key == "space" {
            self.interaction.space_held = false;
            cx.notify();
        }
    }

    pub(crate) fn on_modifiers_changed(
        &mut self,
        ev: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_voice_recording_shortcut() && is_voice_hold_modifier_release(&ev.modifiers) {
            self.end_voice_recording(VoiceTrigger::Shortcut, window, cx);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/app_keyboard_tests.rs"]
mod tests;
