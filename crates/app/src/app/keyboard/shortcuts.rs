use super::*;

impl MultisoulApp {
    pub(super) fn dispatch_shortcut_command(
        &mut self,
        command: ShortcutCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            ShortcutCommandId::FocusCommandInput => self.focus_command_input(window, cx),
            ShortcutCommandId::SavePreviewFile => {
                if self
                    .desktop_workspace
                    .content_tabs
                    .iter()
                    .any(|t| t.selected && t.kind == storage::DesktopContentKind::ProjectFiles)
                {
                    self.save_workbench_file(cx);
                    return;
                }
                if let Some(node_id) = current_selection_node_id(&self.runtime) {
                    self.save_preview_edit(node_id, cx);
                }
            }
            ShortcutCommandId::NewWindow => self.on_new_window(&crate::app::NewWindow, window, cx),
            ShortcutCommandId::NewWorkspace
            | ShortcutCommandId::AddProject
            | ShortcutCommandId::SwitchWorkspace
            | ShortcutCommandId::PreviousWorkspace
            | ShortcutCommandId::NextWorkspace
            | ShortcutCommandId::PreviousProject
            | ShortcutCommandId::NextProject
            | ShortcutCommandId::ExpandSelectedProject
            | ShortcutCommandId::ShowProjectOverview
            | ShortcutCommandId::ShowWorkspaceDirectory => {
                self.navigate_desktop_shortcut(command, window, cx);
            }
            ShortcutCommandId::NewTerminal => {
                self.on_new_terminal(&crate::app::NewTerminal, window, cx)
            }
            ShortcutCommandId::NewBrowser => {
                self.on_new_browser(&crate::app::NewBrowser, window, cx)
            }
            ShortcutCommandId::OpenFileWorkbench => self.open_file_workbench(cx),
            ShortcutCommandId::ShowChanges => self.open_changes_view(cx),
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
            ShortcutCommandId::NewInfCodeAgent => {
                self.on_new_infcode_agent(&crate::app::NewInfCodeAgent, window, cx)
            }
            ShortcutCommandId::NewGrokAgent => {
                self.on_new_grok_agent(&crate::app::NewGrokAgent, window, cx)
            }
            ShortcutCommandId::NewKimiAgent => {
                self.on_new_kimi_agent(&crate::app::NewKimiAgent, window, cx)
            }
            ShortcutCommandId::NewZCodeAgent => {
                self.on_new_zcode_agent(&crate::app::NewZCodeAgent, window, cx)
            }
            ShortcutCommandId::NewPiAgent => {
                self.on_new_pi_agent(&crate::app::NewPiAgent, window, cx)
            }
            ShortcutCommandId::NewHermesAgent => {
                self.on_new_hermes_agent(&crate::app::NewHermesAgent, window, cx)
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
            ShortcutCommandId::ToggleRunOverview => self.toggle_run_overview(window, cx),
            ShortcutCommandId::ToggleIdeas => self.toggle_ideas(window, cx),
            ShortcutCommandId::ToggleBrainTrace => self.toggle_brain_trace(window, cx),
            ShortcutCommandId::AssignIdeaToCard(_) | ShortcutCommandId::AssignIdeaToNewAgent(_) => {
                self.assign_idea_by_shortcut(command, window, cx)
            }
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
