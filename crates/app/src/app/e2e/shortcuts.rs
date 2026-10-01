use crate::shortcuts::ShortcutCommandId;

pub(super) fn shortcut_command_name(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::FocusCommandInput => "focus_command_input",
        ShortcutCommandId::NewWindow => "new_window",
        ShortcutCommandId::NewTerminal => "new_terminal",
        ShortcutCommandId::NewBrowser => "new_browser",
        ShortcutCommandId::CopyCardContent => "copy_card_content",
        ShortcutCommandId::OpenProject => "open_project",
        ShortcutCommandId::OpenPreviewFile => "open_preview_file",
        ShortcutCommandId::SavePreviewFile => "save_preview_file",
        ShortcutCommandId::NewClaudeAgent => "new_claude_agent",
        ShortcutCommandId::NewCodexAgent => "new_codex_agent",
        ShortcutCommandId::NewCursorAgent => "new_cursor_agent",
        ShortcutCommandId::NewOpenCodeAgent => "new_opencode_agent",
        ShortcutCommandId::NewGrokAgent => "new_grok_agent",
        ShortcutCommandId::DeleteCard => "delete_card",
        ShortcutCommandId::ReflowLayout => "reflow_layout",
        ShortcutCommandId::ToggleMasterLayout => "toggle_master_layout",
        ShortcutCommandId::ExitFocus => "exit_focus",
        ShortcutCommandId::ToggleSettings => "toggle_settings",
        ShortcutCommandId::VoiceInteraction => "voice_interaction",
    }
}
