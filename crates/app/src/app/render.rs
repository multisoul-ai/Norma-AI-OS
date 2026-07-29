use super::*;

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_active = window.is_window_active();
        self.voice_window_inset_window_visible = state::window_visible_for_voice_inset(window);
        self.sync_voice_window_inset_enabled();
        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_window))
            .on_action(cx.listener(Self::on_open_project))
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_browser_command))
            .on_action(cx.listener(Self::on_ego_card_command))
            .on_action(cx.listener(Self::on_send_tab_to_focused_pty))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .size_full()
            .relative()
            .bg(gpui::rgba(self.theme.graphite))
            .text_size(px(13.0))
            .when(matches!(self.screen, AppScreen::Canvas), |root| {
                root.on_action(cx.listener(Self::on_new_terminal))
                    .on_action(cx.listener(Self::on_new_browser))
                    .on_action(cx.listener(Self::on_new_ego))
                    .on_action(cx.listener(Self::on_new_claude_agent))
                    .on_action(cx.listener(Self::on_new_codex_agent))
                    .on_action(cx.listener(Self::on_new_cursor_agent))
                    .on_action(cx.listener(Self::on_new_grok_agent))
                    .on_action(cx.listener(Self::on_new_opencode_agent))
                    .on_action(cx.listener(Self::on_copy_card_content))
                    .on_action(cx.listener(Self::on_open_preview_file))
                    .on_action(cx.listener(Self::on_reflow_layout))
                    .on_action(cx.listener(Self::on_toggle_master_layout))
                    .on_action(cx.listener(Self::on_toggle_settings))
            })
            .child(crate::app::ime::TerminalImeElement::new(
                self.focus_handle.clone(),
                cx.weak_entity(),
            ));

        match self.screen {
            AppScreen::Launchpad => {
                root = root.child(self.render_launchpad(window, cx));
            }
            AppScreen::Canvas => {
                #[cfg(target_os = "macos")]
                self.sync_canvas_video_wallpaper(cx);
                self.reflow_if_window_size_changed(window);
                root = root
                    .child(self.render_workspace_backdrop())
                    .child(render_canvas(self, window, cx))
                    .child(self.render_workspace_chrome(window, cx));
                if let Some(error_banner) = self.render_workspace_error_banner(cx) {
                    root = root.child(error_banner);
                }
                if let Some(visual) = self.voice_window_inset.visual() {
                    root = root.child(voice_window_inset::render(visual));
                }
                if self.settings_panel.open {
                    root = root.child(self.render_settings_panel(window, cx));
                }
            }
        }

        root
    }
}
