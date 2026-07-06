//! Public e2e-facing app snapshots.
//!
//! These accessors keep external e2e tests on stable, primitive state without
//! exposing the full private settings model.

use super::MultisoulApp;
use crate::shortcuts::ShortcutCommandId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsE2eSnapshot {
    pub settings_open: bool,
    pub active_section: &'static str,
    pub recording_command: Option<&'static str>,
    pub new_terminal_shortcut: Option<String>,
    pub new_terminal_modified: bool,
    pub status_message: Option<String>,
}

impl MultisoulApp {
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
        }
    }
}

fn shortcut_command_name(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::NewWindow => "new_window",
        ShortcutCommandId::NewTerminal => "new_terminal",
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
