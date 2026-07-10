//! Public e2e-facing app snapshots.
//!
//! These accessors keep external e2e tests on stable, primitive state without
//! exposing the full private settings model.

use super::{AppScreen, MultisoulApp};
use crate::settings::agent_hooks::AgentHookServer;
use crate::shortcuts::ShortcutCommandId;
use crate::shortcuts::ShortcutSettingsState;
use gpui::Context;
use runtime::Runtime;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsE2eSnapshot {
    pub settings_open: bool,
    pub active_section: &'static str,
    pub recording_command: Option<&'static str>,
    pub new_terminal_shortcut: Option<String>,
    pub new_terminal_modified: bool,
    pub status_message: Option<String>,
    pub agent_provider: &'static str,
}

impl MultisoulApp {
    pub fn new_with_pty_factory(cx: &mut Context<Self>, factory: runtime::PtyFactory) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_pty_factory(factory);
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        let mut app = Self::build(cx, runtime, agent_hook_server);
        app.shortcuts = ShortcutSettingsState::default();
        app.voice_settings = crate::settings::voice::VoiceSettingsState::in_memory_default();
        app
    }

    pub fn screen(&self) -> AppScreen {
        self.screen
    }

    pub fn enter_canvas_for_test(&mut self, root: &std::path::Path, cx: &mut Context<Self>) {
        self.runtime.set_workspace_root(root.to_path_buf());
        self.screen = AppScreen::Canvas;
        self.last_reflow_window_size = None;
        cx.notify();
    }

    pub fn settings_e2e_snapshot(&self) -> SettingsE2eSnapshot {
        let new_terminal = self
            .shortcuts
            .effective
            .iter()
            .find(|entry| entry.command == ShortcutCommandId::NewTerminal);
        SettingsE2eSnapshot {
            settings_open: self.settings_panel.open,
            active_section: self.settings_panel.active_section.title(),
            recording_command: self.shortcuts.recording.map(shortcut_command_name),
            new_terminal_shortcut: new_terminal.and_then(|entry| entry.current_keystroke.clone()),
            new_terminal_modified: new_terminal.map(|entry| entry.modified).unwrap_or(false),
            status_message: self.shortcuts.status.message.clone(),
            agent_provider: self.voice_settings.values().agent_provider().as_str(),
        }
    }
}

fn shortcut_command_name(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::NewWindow => "new_window",
        ShortcutCommandId::NewTerminal => "new_terminal",
        ShortcutCommandId::NewBrowser => "new_browser",
        ShortcutCommandId::CopyCardContent => "copy_card_content",
        ShortcutCommandId::OpenProject => "open_project",
        ShortcutCommandId::OpenPreviewFile => "open_preview_file",
        ShortcutCommandId::NewClaudeAgent => "new_claude_agent",
        ShortcutCommandId::NewCodexAgent => "new_codex_agent",
        ShortcutCommandId::NewOpenCodeAgent => "new_opencode_agent",
        ShortcutCommandId::DeleteCard => "delete_card",
        ShortcutCommandId::ReflowLayout => "reflow_layout",
        ShortcutCommandId::ToggleMasterLayout => "toggle_master_layout",
        ShortcutCommandId::ExitFocus => "exit_focus",
        ShortcutCommandId::ToggleSettings => "toggle_settings",
    }
}
