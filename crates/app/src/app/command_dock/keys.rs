use crate::{
    app::{AppScreen, MultisoulApp},
    shortcuts::ShortcutCommandId,
};
use gpui::{Context, EntityInputHandler, KeyDownEvent, Window};

impl MultisoulApp {
    pub(super) fn command_input_composing(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.command_dock.as_ref().is_some_and(|state| {
            state.input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        })
    }

    pub(crate) fn complete_command_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_input_composing(window, cx) {
            return;
        }
        let Some(state) = &self.command_dock else {
            return;
        };
        if let Some(item) = state
            .suggestions()
            .get(state.selected)
            .filter(|_| state.visible)
        {
            self.set_command_query(item.completion.clone(), true, window, cx);
        } else {
            window.focus_next();
        }
    }

    pub(crate) fn capture_command_dock_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The same InputState is rendered in the sidebar and the project Dock. When
        // focused from the sidebar, the project Scene is not on the keyboard path,
        // so the host forwards command navigation to the owning project.
        if let Some(project) = self.expanded_desktop_project() {
            // Enter is delivered by InputState::PressEnter. Routing it through the
            // host here would nest the project's dispatch back into an updating host.
            if event.keystroke.key != "enter" {
                project.update(cx, |project, cx| {
                    project.capture_command_dock_key(event, window, cx);
                });
            }
            return;
        }
        if !matches!(
            self.screen,
            AppScreen::Canvas | AppScreen::DesktopWorkspace | AppScreen::Launchpad
        ) || self.settings_panel.open
            || self.feature_guide.is_visible()
        {
            return;
        }
        if self.shortcuts.command_for_keystroke(&event.keystroke)
            == Some(ShortcutCommandId::FocusCommandInput)
        {
            self.focus_command_input(window, cx);
            cx.stop_propagation();
            return;
        }
        if !self.command_input_focused(window, cx)
            || self.command_input_composing(window, cx)
            || event.keystroke.modifiers.modified()
        {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "tab" | "enter" | "escape") {
            return;
        }
        cx.stop_propagation();
        let Some(state) = &mut self.command_dock else {
            return;
        };
        let items = state.suggestions();
        match key {
            "up" | "down" => {
                if state.visible && !items.is_empty() {
                    state.selected = if key == "down" {
                        (state.selected + 1) % items.len()
                    } else {
                        (state.selected + items.len() - 1) % items.len()
                    };
                }
                state.scroll.scroll_to_item(state.selected);
                state.set_visible(true);
            }
            "tab" => self.complete_command_input(window, cx),
            "enter" => self.execute_dock_suggestion(window, cx),
            "escape" => {
                if !state.visible {
                    self.focus_handle.focus(window);
                }
                state.set_visible(false);
            }
            _ => {}
        }
        cx.notify();
    }
}
