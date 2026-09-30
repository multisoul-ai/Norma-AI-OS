use super::*;

impl MultisoulApp {
    pub(super) fn dispatch_shortcut_command(
        &mut self,
        command: ShortcutCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            ShortcutCommandId::SavePreviewFile => {
                if let Some(node_id) = current_selection_node_id(&self.runtime) {
                    self.save_preview_edit(node_id, cx);
                }
            }
            ShortcutCommandId::NewWindow => self.on_new_window(&crate::app::NewWindow, window, cx),
            ShortcutCommandId::NewTerminal => {
                self.on_new_terminal(&crate::app::NewTerminal, window, cx)
            }
            ShortcutCommandId::NewBrowser => {
                self.on_new_browser(&crate::app::NewBrowser, window, cx)
            }
            ShortcutCommandId::CopyCardContent => {
                self.on_copy_card_content(&crate::app::CopyCardContent, window, cx)
            }
            ShortcutCommandId::OpenProject => {
                self.on_open_project(&crate::app::OpenProject, window, cx)
            }
            ShortcutCommandId::OpenPreviewFile => {
                self.on_open_preview_file(&crate::app::OpenPreviewFile, window, cx)
            }
            ShortcutCommandId::NewClaudeAgent => {
                self.on_new_claude_agent(&crate::app::NewClaudeAgent, window, cx)
            }
            ShortcutCommandId::NewCodexAgent => {
                self.on_new_codex_agent(&crate::app::NewCodexAgent, window, cx)
            }
            ShortcutCommandId::NewCursorAgent => {
                self.on_new_cursor_agent(&crate::app::NewCursorAgent, window, cx)
            }
            ShortcutCommandId::NewOpenCodeAgent => {
                self.on_new_opencode_agent(&crate::app::NewOpenCodeAgent, window, cx)
            }
            ShortcutCommandId::NewGrokAgent => {
                self.on_new_grok_agent(&crate::app::NewGrokAgent, window, cx)
            }
            ShortcutCommandId::DeleteCard => self.on_delete_card_shortcut(window, cx),
            ShortcutCommandId::ReflowLayout => {
                self.on_reflow_layout(&crate::app::ReflowLayout, window, cx)
            }
            ShortcutCommandId::ToggleMasterLayout => {
                self.on_toggle_master_layout(&crate::app::ToggleMasterLayout, window, cx)
            }
            ShortcutCommandId::ExitFocus => self.on_exit_focus(&crate::app::ExitFocus, window, cx),
            ShortcutCommandId::ToggleSettings => self.toggle_settings_panel(cx),
            ShortcutCommandId::VoiceInteraction => {
                if !matches!(
                    self.voice_settings.readiness(),
                    crate::settings::voice::VoiceEntryState::Ready
                ) {
                    self.open_voice_settings(window, cx);
                    return;
                }
                self.activate_ready_voice(false, window, cx);
            }
        }
    }
}
