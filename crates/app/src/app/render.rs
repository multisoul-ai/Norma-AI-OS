use super::*;
use canvas::CanvasNodeKind;

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_active = window.is_window_active();
        self.sync_feature_guide_blockers(cx);
        self.voice_window_inset_window_visible = state::window_visible_for_voice_inset(window);
        self.sync_voice_window_inset_enabled();
        let performance_state = self.performance_state();
        self.performance.set_state(performance_state);
        self.performance.record_repaint();
        self.retain_visible_dispatch_receipts(window);
        self.start_animation_frame_driver_if_needed(window, cx);
        let feature_guide_visible = self.feature_guide.is_visible();
        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_open_feature_guide))
            .when(!feature_guide_visible, |root| {
                root.on_action(cx.listener(Self::on_new_window))
                    .on_action(cx.listener(Self::on_open_project))
                    .on_action(cx.listener(Self::on_clone_project))
                    .on_action(cx.listener(Self::on_exit_focus))
                    .on_action(cx.listener(Self::on_browser_command))
                    .on_action(cx.listener(Self::on_ego_card_command))
            })
            .on_action(cx.listener(Self::on_send_tab_to_focused_pty))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .size_full()
            .relative()
            .bg(gpui::rgba(self.theme.graphite))
            .text_size(px(13.0))
            .when(
                matches!(self.screen, AppScreen::Canvas) && !feature_guide_visible,
                |root| {
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
                },
            )
            .child(crate::app::ime::TerminalImeElement::new(
                self.focus_handle.clone(),
                cx.weak_entity(),
            ));

        match self.screen {
            AppScreen::Launchpad => {
                root = root.child(self.render_launchpad(window, cx));
            }
            AppScreen::Canvas => {
                self.performance.record_render();
                #[cfg(target_os = "macos")]
                self.sync_canvas_video_wallpaper(cx);
                self.reflow_if_window_size_changed(window);
                root = root
                    .child(self.render_workspace_backdrop())
                    .child(render_canvas(self, window, cx))
                    .when_some(
                        crate::effects::dispatch_receipt::render(self, window),
                        |root, effect| root.child(effect),
                    )
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

        if self.clone_project.is_open() {
            root = root.child(self.render_clone_project_dialog(window, cx));
        }
        if self.feature_guide.is_visible() {
            root = root.child(self.render_feature_guide(window, cx));
        }

        root
    }
}

impl MultisoulApp {
    fn performance_state(&self) -> crate::performance_monitor::PerformanceState {
        use crate::performance_monitor::PerformanceState;

        if !self.window_active {
            return PerformanceState::Background;
        }
        let activity = self.animation_activity();
        if activity.voice_active {
            return PerformanceState::VoiceActive;
        }
        #[cfg(target_os = "macos")]
        let video_animation_active = self.video_wallpaper.is_some();
        #[cfg(not(target_os = "macos"))]
        let video_animation_active = false;
        // 普通 Terminal 光标闪烁属于 idle 基线，不单独提升为 animation；否则
        // 七张空闲 Terminal 的 L3 场景会永远无法产生 state=idle 样本。
        if activity.canvas_motion_active || video_animation_active {
            return PerformanceState::Animation;
        }
        PerformanceState::Idle
    }

    fn retain_visible_dispatch_receipts(&mut self, window: &mut Window) {
        if !self.motion.has_dispatch_receipts() {
            return;
        }
        if !matches!(self.screen, AppScreen::Canvas) || !self.window_active {
            self.motion.retain_dispatch_targets(&[]);
            return;
        }
        let visible_area = self.visible_canvas_rect(window);
        let mut target_node_ids = self.motion.dispatch_target_node_ids();
        target_node_ids.retain(|node_id| {
            self.runtime
                .document
                .node(*node_id)
                .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
                .is_some_and(|node| {
                    let rect = self.motion.current_node_rect(node.id, node.bounds);
                    visible_area.contains(canvas::Vec2::new(
                        rect.origin.x + rect.size.x / 2.0,
                        rect.origin.y + rect.size.y / 2.0,
                    ))
                })
        });
        self.motion.retain_dispatch_targets(&target_node_ids);
    }
}
