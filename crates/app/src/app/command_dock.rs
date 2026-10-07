//! Command Dock input state and adaptation to existing app actions.
mod conversation;
mod keys;
mod motion;
mod view;
pub(crate) use view::DOCK_RADIUS;

use super::{AppScreen, MultisoulApp, NewBrowser, NewTerminal};
use gpui::{App, AppContext, Context, Entity, Focusable, ScrollHandle, Subscription, Window};
use runtime::command::{suggestions, CommandSuggestion, CommandTheme, DockCommand};
use ui::{
    component::input::{InputEvent, InputState},
    ThemeId,
};

pub(crate) struct CommandDockState {
    pub input: Entity<InputState>,
    _subscription: Subscription,
    pub draft: String,
    pub selected: usize,
    pub scroll: ScrollHandle,
    pub visible: bool,
    pub drawer: motion::CommandDrawer,
    pub pending_preview: Option<String>,
    pub error: Option<ui::i18n::LocalizedText>,
    pub reply: Option<ui::i18n::LocalizedText>,
    pub pending_reply: bool,
    pub reply_generation: u64,
}

impl CommandDockState {
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible && !self.draft.trim().is_empty();
    }

    fn suggestions(&self) -> Vec<CommandSuggestion> {
        Self::suggestions_for(&self.draft)
    }

    fn suggestions_for(draft: &str) -> Vec<CommandSuggestion> {
        suggestions(
            draft,
            &ThemeId::ALL.map(|id| CommandTheme {
                id: id.as_str(),
                title: id.display_name(),
                dark: id.is_dark(),
            }),
        )
    }
}

impl MultisoulApp {
    pub(crate) fn command_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.command_dock
            .as_ref()
            .is_some_and(|state| state.input.focus_handle(cx).is_focused(window))
    }

    pub(crate) fn ensure_command_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_dock.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "chrome.talk_to_norma_or_type_for_commands",
                window,
                cx,
            ))
        });
        let subscription = cx.subscribe_in(&input, window, |app, input, event, window, cx| {
            match event {
                InputEvent::Change => {
                    let value = input.read(cx).value().to_string();
                    let Some(state) = &mut app.command_dock else {
                        return;
                    };
                    // Programmatic completion/clearing already updated the draft and visibility.
                    if state.draft == value {
                        return;
                    }
                    state.draft = value;
                    state.selected = 0;
                    state.scroll.scroll_to_item(0);
                    app.interaction.layout_menu_open = false;
                    state.set_visible(true);
                    state.error = None;
                    state.reply = None;
                    state.pending_reply = false;
                    state.reply_generation += 1;
                }
                InputEvent::Focus => {
                    app.release_all_browser_input();
                    super::native_input::focus_gpui(window, "command_input_focus");
                    app.runtime.focus.exit_to_canvas();
                    app.interaction.space_held = false;
                    app.interaction.layout_menu_open = false;
                    app.interaction.close_new_menu();
                    if let Some(state) = &mut app.command_dock {
                        state.set_visible(true);
                    }
                }
                InputEvent::Blur => app.hide_command_suggestions(),
                InputEvent::PressEnter { .. } => {
                    if !app.command_input_composing(window, cx) {
                        app.execute_dock_suggestion(window, cx);
                    }
                }
            }
            cx.notify();
        });
        self.command_dock = Some(CommandDockState {
            input,
            _subscription: subscription,
            draft: String::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            visible: false,
            drawer: motion::CommandDrawer::default(),
            pending_preview: None,
            error: None,
            reply: None,
            pending_reply: false,
            reply_generation: 0,
        });
    }

    pub(crate) fn focus_command_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(
            self.screen,
            AppScreen::Canvas | AppScreen::DesktopWorkspace | AppScreen::Launchpad
        ) || self.settings_panel.open
            || self.feature_guide.is_visible()
        {
            return;
        }
        self.ensure_command_input(window, cx);
        self.release_all_browser_input();
        super::native_input::focus_gpui(window, "command_input_shortcut");
        self.runtime.focus.exit_to_canvas();
        self.interaction.space_held = false;
        self.interaction.layout_menu_open = false;
        self.interaction.close_new_menu();
        if let Some(state) = &mut self.command_dock {
            state.set_visible(true);
            state.input.focus_handle(cx).focus(window);
        }
        cx.notify();
    }

    pub(crate) fn hide_command_suggestions(&mut self) {
        if let Some(state) = &mut self.command_dock {
            state.set_visible(false);
        }
    }

    fn set_command_query(
        &mut self,
        value: String,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = &mut self.command_dock else {
            return;
        };
        state.draft = value.clone();
        state.selected = 0;
        state.scroll.scroll_to_item(0);
        state.set_visible(visible);
        state.error = None;
        state.reply = None;
        state.pending_reply = false;
        state.reply_generation += 1;
        state
            .input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        cx.notify();
    }

    fn execute_dock_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &self.command_dock else {
            return;
        };
        if !state.visible || self.command_input_composing(window, cx) {
            return;
        }
        let Some(candidate) = state.suggestions().get(state.selected).cloned() else {
            if !state.draft.trim().starts_with('/') {
                self.submit_dock_conversation(window, cx);
            }
            return;
        };
        let Some(command) = candidate.command else {
            self.set_command_query(candidate.completion, true, window, cx);
            return;
        };
        if command == DockCommand::Preview {
            if let Some(state) = &mut self.command_dock {
                // The native picker is asynchronous; only another preview must wait for it.
                if state.pending_preview.is_some() {
                    state.error = Some(ui::i18n::LocalizedText::new(
                        "chrome.finish_or_cancel_the_open_file_picker_first",
                    ));
                    cx.notify();
                    return;
                }
                state.pending_preview = Some(state.draft.clone());
                state.set_visible(false);
            }
            self.prompt_preview_file_from_command(window, cx);
            return;
        }
        if let DockCommand::Theme(name) = command {
            if let Some(id) = ThemeId::ALL.into_iter().find(|id| id.as_str() == name) {
                self.set_theme(id, cx);
                self.set_command_query(String::new(), false, window, cx);
            }
            return;
        }
        let before = self.runtime.document.nodes().len();
        let desktop_agent_pending = matches!(command, DockCommand::Agent(_))
            && self
                .desktop_host
                .as_ref()
                .and_then(|host| host.upgrade())
                .is_some();
        match command {
            DockCommand::Agent(profile) => self.spawn_agent_card(profile, window, cx),
            DockCommand::Terminal => self.on_new_terminal(&NewTerminal, window, cx),
            DockCommand::Browser => self.on_new_browser(&NewBrowser, window, cx),
            DockCommand::Preview | DockCommand::Theme(_) => unreachable!(),
        }
        // Desktop Agent creation validates its directory asynchronously. Consume the
        // accepted command now; a second Enter event must not submit it again.
        // Validation/spawn failures are reported by the desktop host.
        if desktop_agent_pending || self.runtime.document.nodes().len() > before {
            self.set_command_query(String::new(), false, window, cx);
            self.focus_handle.focus(window);
        } else if let Some(state) = &mut self.command_dock {
            state.error = Some(self.workspace_state.error.clone().unwrap_or_else(|| {
                ui::i18n::LocalizedText::new(
                    "chrome.could_not_create_the_card_check_the_runtime_environment_and_try",
                )
            }));
            cx.notify();
        }
    }

    pub(crate) fn finish_command_preview(
        &mut self,
        success: bool,
        error: Option<ui::i18n::LocalizedText>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = &mut self.command_dock {
            // A late picker result belongs to its original query, not a newer command.
            if state.pending_preview.take().as_ref() != Some(&state.draft) {
                cx.notify();
                return;
            }
            state.set_visible(!success);
            state.error = error;
        }
        if success {
            self.set_command_query(String::new(), false, window, cx);
            self.focus_handle.focus(window);
        } else if let Some(state) = &self.command_dock {
            state.input.focus_handle(cx).focus(window);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/command_dock_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/command_panel_theme_tests.rs"]
mod panel_theme_tests;
