use super::*;
use canvas::CanvasNodeKind;

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_active = window.is_window_active();
        if self.window_active != window_active {
            self.window_active = window_active;
        }
        if !matches!(self.screen, AppScreen::Canvas) {
            // 离开 Canvas 后丢弃局部 Snake Entity；其 on_next_frame 回调随 Entity
            // 生命周期结束，不能在 Launchpad 后台继续唤醒。
            self.working_snake_views.clear();
        }
        self.sync_feature_guide_blockers(cx);
        self.sync_command_drawer(window);
        self.voice_window_inset_window_visible = state::window_visible_for_voice_inset(window);
        self.sync_voice_window_inset_enabled();
        let performance_state = self.performance_state(cx);
        self.performance.set_state(performance_state);
        self.performance.record_repaint();
        self.retain_visible_dispatch_receipts(window);
        let ordered_snake_views: Vec<_> = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| self.working_snake_views.get(&node.id).cloned())
            .collect();
        self.start_animation_frame_driver_if_needed(window, cx);
        let feature_guide_visible = self.feature_guide.is_visible();
        let desktop_project_zoom = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .is_some_and(|host| host.read(cx).desktop_project_zoom_active());
        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::capture_command_dock_key))
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
            .when(self.desktop_host.is_none(), |root| {
                root.bg(gpui::rgba(self.theme.graphite))
            })
            .text_size(px(13.0))
            .when(
                matches!(self.screen, AppScreen::Canvas) && !feature_guide_visible,
                |root| {
                    root.on_action(cx.listener(Self::on_new_terminal))
                        .on_action(cx.listener(Self::on_preview_image_command))
                        .on_action(cx.listener(Self::on_preview_edit_command))
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
                if self.desktop_workspace_zoom_active() {
                    root = root.child(self.render_workspace_backdrop());
                }
                root = root.child(self.render_desktop_launchpad(window, cx));
                if self.desktop_workspace_zoom_active() {
                    root = root.child(self.render_desktop_workspace(window, cx));
                }
            }
            AppScreen::DesktopWorkspace => {
                if self.desktop_workspace_zoom_active() {
                    root = root.child(self.render_workspace_backdrop());
                    root = root.child(self.render_desktop_launchpad(window, cx));
                }
                root = root.child(self.render_desktop_workspace(window, cx));
            }
            AppScreen::Canvas => {
                root = root
                    .child(
                        gpui::AnyView::from(self.canvas_surface.clone())
                            .cached(gpui::StyleRefinement::default().absolute().size_full()),
                    )
                    .children(ordered_snake_views);
                if !desktop_project_zoom && self.desktop_host.is_none() {
                    root = root
                        .child(render_canvas_overlays(self, window, cx))
                        .when_some(
                            crate::effects::dispatch_receipt::render(self, window),
                            |root, effect| root.child(effect),
                        )
                        .child(self.render_workspace_chrome(window, cx));
                    if let Some(error_banner) = self.render_workspace_error_banner(cx) {
                        root = root.child(error_banner);
                    }
                    root = root.child(self.voice_window_inset_view.clone());
                }
            }
        }

        if matches!(self.screen, AppScreen::DesktopWorkspace) && !feature_guide_visible {
            root = root.child(self.render_desktop_dock(window, cx));
        }

        if self.feature_guide.should_render() {
            root = root.child(self.render_feature_guide(window, cx));
        }
        if self.settings_panel.open {
            root = root.child(self.render_settings_panel(window, cx));
        }
        if self.clone_project.is_open() {
            root = root.child(self.render_clone_project_dialog(window, cx));
        }
        if self.desktop_workspace.rename_target.is_some() {
            root = root.child(self.render_desktop_rename_dialog(cx));
        }
        if self.desktop_workspace.switcher_open
            && matches!(self.screen, AppScreen::Launchpad)
            && !self.desktop_workspace_zoom_active()
        {
            root = root.child(self.render_desktop_switcher(cx));
        }
        if self.desktop_workspace.pending_delete.is_some() {
            root = root.child(self.render_desktop_delete_confirmation(cx));
        }
        if self.desktop_workspace.pending_project_path.is_some() {
            root = root.child(self.render_desktop_project_destination(cx));
        }
        // Project previews share this window's Root; only the host owns its modal layer.
        if self.desktop_host.is_none() {
            if let Some(dialogs) = ui::component::Root::render_dialog_layer(window, cx) {
                root = root.child(dialogs);
            }
        }

        root
    }
}

impl MultisoulApp {
    pub(crate) fn render_canvas_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.performance.record_render();
        #[cfg(target_os = "macos")]
        if self.desktop_host.is_none() {
            self.sync_canvas_video_wallpaper(cx);
        }
        self.reflow_if_window_size_changed(window);
        div()
            .id("canvas-surface")
            .debug_selector(|| "canvas-surface".to_string())
            .relative()
            .size_full()
            .when(self.desktop_host.is_none(), |this| {
                this.child(self.render_workspace_backdrop())
            })
            .child(render_canvas(self, window, cx))
    }

    fn performance_state(&self, cx: &gpui::App) -> crate::performance_monitor::PerformanceState {
        use crate::performance_monitor::PerformanceState;

        if !self.window_active {
            return PerformanceState::Background;
        }
        let activity = self.animation_activity();
        if activity.voice_active || self.voice_window_inset_events.is_animation_active() {
            return PerformanceState::VoiceActive;
        }
        #[cfg(target_os = "macos")]
        let video_animation_active = self.video_wallpaper.is_some();
        #[cfg(not(target_os = "macos"))]
        let video_animation_active = false;
        // 普通 Terminal 光标闪烁属于 idle 基线，不单独提升为 animation；否则
        // 七张空闲 Terminal 的 L3 场景会永远无法产生 state=idle 样本。
        let working_snake_active = self
            .working_snake_views
            .values()
            .any(|view| view.read(cx).is_active_animation());
        if activity.canvas_motion_active || working_snake_active || video_animation_active {
            return PerformanceState::Animation;
        }
        PerformanceState::Idle
    }

    fn retain_visible_dispatch_receipts(&mut self, window: &mut Window) {
        if !self.motion.has_dispatch_receipts() {
            return;
        }
        if !matches!(self.screen, AppScreen::Canvas) {
            self.motion.retain_dispatch_endpoints(&[]);
            return;
        }
        let visible_area = self.visible_canvas_rect(window);
        let mut endpoint_node_ids = self.motion.dispatch_endpoint_node_ids();
        endpoint_node_ids.retain(|node_id| {
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
        self.motion.retain_dispatch_endpoints(&endpoint_node_ids);
    }
}
