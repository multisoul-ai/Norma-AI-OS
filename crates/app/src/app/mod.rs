use crate::app::browser::{BrowserAddressEdit, BrowserStore};
use crate::app::frame_throttle::FrameThrottle;
#[cfg(test)]
use crate::app::frame_throttle::CURSOR_REPAINT_INTERVAL;
use crate::canvas::interaction::InteractionState;
use crate::canvas::master_layout::{
    apply_workspace_reflow_for_area, close_node_and_update_layout_for_area, layout_gap_for_zoom,
    reapply_master_layout_for_area, term_grid_size_for_card, MasterLayoutState,
};
#[cfg(test)]
use crate::canvas::master_layout::{
    close_node_and_reflow_for_area, workspace_reflow_config_for_zoom,
};
use crate::canvas::spawn_focus;
use crate::canvas::view::render_canvas;
use crate::cards::agent::card::agent_chase_intent;
use crate::motion::MotionStore;
use crate::norma::server::NormaServer;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::agents::AgentDefinitionsState;
use crate::settings::panel::SettingsPanelState;
use crate::settings::voice::{VoiceSettingsInputs, VoiceSettingsState};
use crate::settings::wallpaper::WallpaperSettingsState;
use crate::shortcuts::ShortcutSettingsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use canvas::{CanvasNodeKind, CanvasRect, NodeId, NodeType, Vec2};
use gpui::{actions, div, prelude::*, px, ClickEvent, Context, FocusHandle, Focusable, Window};
use runtime::{route, AgentTaskState, InputEvent, NodeRunState, Route, Runtime};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
#[cfg(test)]
use std::time::Duration;
use ui::{Theme, ThemeId};
use voice_window_inset::VoiceWindowInset;
mod agent_actions;
mod agent_announcements;
mod agent_close;
mod agent_runtime;
mod bootstrap;
pub(crate) mod browser;
pub(crate) use browser::{BrowserCommand, BrowserCommandKind};
pub(crate) mod cef_process;
mod chrome;
mod clipboard_image;
mod cursor_animation;
pub(crate) mod dialogue;
mod e2e;
mod file_picker;
mod frame_loop;
mod frame_throttle;
mod ime;
mod ime_geometry;
mod keyboard;
mod keyboard_settings;
mod lifecycle;
mod new_menu;
mod state;
#[cfg(target_os = "macos")]
mod video_decoder;
#[cfg(target_os = "macos")]
pub(crate) mod video_thumbnails;
#[cfg(target_os = "macos")]
pub(crate) mod video_wallpaper;
mod voice;
mod voice_window_inset;
mod wallpaper_actions;
pub(crate) mod wallpaper_resolvers;
pub use e2e::{
    AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot, ApiKeyDisplayState,
    InitializationAnimationE2eSnapshot, SettingsE2eSnapshot,
};
pub(crate) use state::mark_runtime_node_viewed_if_needed;
pub use state::AppScreen;
actions!(
    multisoul,
    [
        NewWindow,
        NewTerminal,
        NewClaudeAgent,
        NewCodexAgent,
        NewCursorAgent,
        NewOpenCodeAgent,
        NewBrowser,
        CopyCardContent,
        OpenPreviewFile,
        OpenProject,
        ExitFocus,
        ReflowLayout,
        ToggleMasterLayout,
        ToggleSettings,
        QuitNormaOs
    ]
);
const WALLPAPER_IMAGE_OPACITY: f32 = 1.0;

pub struct MultisoulApp {
    pub runtime: Runtime,
    pub theme_id: ThemeId,
    pub theme: Theme,
    pub interaction: InteractionState,
    pub(crate) motion: MotionStore,
    pub(crate) previews: PreviewStore,
    pub(crate) browsers: BrowserStore,
    pub(crate) browser_address_edit: Option<BrowserAddressEdit>,
    pub(crate) master_layout: MasterLayoutState,
    pub(crate) persistence: PersistenceScheduler,
    pub(crate) settings_panel: SettingsPanelState,
    pub(crate) agent_definitions: AgentDefinitionsState,
    pub(crate) voice_settings: VoiceSettingsState,
    pub(crate) voice_settings_inputs: VoiceSettingsInputs,
    pub(crate) shortcuts: ShortcutSettingsState,
    pub(crate) norma_server: NormaServer,
    pub(crate) agent_hook_server: AgentHookServer,
    agent_announcements: agent_announcements::AgentAnnouncements,
    agent_announcement_playback: agent_announcements::AgentAnnouncementPlayback,
    pub(crate) screen: AppScreen,
    pub(crate) workspace_state: WorkspaceState,
    pub(crate) recent_project_context_path: Option<PathBuf>,
    pub(crate) voice: crate::app::voice::control::VoiceControlState,
    pub(crate) dialogue: crate::app::dialogue::DialogueControlState,
    pub(crate) tts_preview: crate::app::dialogue::TtsPreviewState,
    pub(crate) tts_selection_generation: u64,
    pub(crate) tts_scan_generation: u64,
    pub(crate) asr_download_cancellations: HashMap<asr::AsrModelId, Arc<AtomicBool>>,
    pub(crate) tts_download_cancellations: HashMap<tts::TtsModelId, Arc<AtomicBool>>,
    pub(crate) voice_ready_notification_visible: bool,
    pub(crate) agent_runtime: Option<agent_runtime::ProjectAgentRuntime>,
    pub(crate) agent_task_coordinator: Option<agent::AgentTaskCoordinator>,
    pub(crate) pending_agent_submissions: agent_runtime::pending::PendingAgentSubmissions,
    pub(crate) pending_agent_close_confirmation: Option<NodeId>,
    pub focus_handle: FocusHandle,
    pub(crate) wallpaper_settings: WallpaperSettingsState,
    #[cfg(target_os = "macos")]
    pub(crate) video_wallpaper: Option<video_wallpaper::VideoWallpaperState>,
    pub(crate) last_reflow_window_size: Option<(f32, f32)>,
    pub(crate) terminal_cursor_phase: f32,
    terminal_cursor_repaint: FrameThrottle,
    voice_window_inset: VoiceWindowInset,
    voice_window_inset_window_visible: bool,
}

impl MultisoulApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        Self::build(cx, runtime, agent_hook_server)
    }

    fn on_new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating terminals.".into());
            cx.notify();
            return;
        }

        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Term, bounds, self.runtime.document.viewport.zoom);
        match self.runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, "spawned terminal node");
                spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
                self.motion.spawn_node(node_id, bounds);
                self.motion.start_initialization(
                    node_id,
                    crate::motion::InitializationAnimationKind::Terminal,
                );
                self.apply_layout_for_current_mode(window);
                self.mark_canvas_dirty();
                self.mark_session_dirty();
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn terminal");
            }
        }
        cx.notify();
    }

    fn on_new_browser(&mut self, _: &NewBrowser, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating browsers.".into());
            cx.notify();
            return;
        }

        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let node_id = self.runtime.spawn_browser(
            crate::cards::browser::normalize_browser_url(""),
            None,
            bounds,
        );
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.motion.spawn_node(node_id, bounds);
        self.apply_layout_for_current_mode(window);
        self.mark_canvas_dirty();
        cx.notify();
    }

    fn on_exit_focus(&mut self, _: &ExitFocus, window: &mut Window, cx: &mut Context<Self>) {
        self.release_all_browser_input();
        self.focus_handle.focus(window);
        self.runtime.focus.exit_to_canvas();
        cx.notify();
    }

    fn on_reflow_layout(&mut self, _: &ReflowLayout, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            return;
        }
        self.reflow_tiled_nodes(window);
        self.mark_canvas_dirty();
        cx.notify();
    }

    fn on_open_project(&mut self, _: &OpenProject, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_open_project(window, cx);
    }

    fn on_new_window(&mut self, _: &NewWindow, _window: &mut Window, cx: &mut Context<Self>) {
        Self::open_new_launchpad_window(cx);
    }

    fn on_open_preview_file(
        &mut self,
        _: &OpenPreviewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_create_preview() {
            self.workspace_state.error = Some("Open a project before previewing files.".into());
            cx.notify();
            return;
        }
        self.prompt_preview_file(window, cx);
    }

    pub(crate) fn remove_node_and_reflow(&mut self, node_id: NodeId, window: &mut Window) -> bool {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let removed_agent_definition_id = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        });
        if self.interaction.keyboard_selected_node == Some(node_id) {
            self.interaction.keyboard_selected_node = None;
        }
        self.interaction.clear_text_selection_for_node(node_id);

        let removed_node_was_browser = self
            .runtime
            .document
            .node(node_id)
            .map(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .unwrap_or(false);
        let next_id = canvas::next_neighbor_after_delete(self.runtime.document.nodes(), node_id);

        let changed = close_node_and_update_layout_for_area(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            node_id,
            area,
            zoom,
        );
        if changed {
            self.previews.remove(node_id);
            if removed_node_was_browser {
                self.release_browser_input(node_id);
                self.focus_handle.focus(window);
            }
            self.remove_browser_view(node_id);
            self.mark_canvas_dirty();
            self.mark_session_dirty();
            if let Some(definition_id) = removed_agent_definition_id {
                if let Err(message) = self
                    .agent_definitions
                    .cleanup_quick_definition(definition_id)
                {
                    self.workspace_state.error = Some(message);
                }
            }

            match next_id {
                Some(next) if self.runtime.document.node(next).is_some() => {
                    self.runtime.selection.select_only(next);
                    self.runtime.focus.exit_to_canvas();
                    self.interaction.keyboard_selected_node = Some(next);
                }
                _ => {
                    self.interaction.keyboard_selected_node = None;
                }
            }
        }
        changed
    }

    pub(crate) fn close_node_through_route(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
    ) -> bool {
        if !self.confirm_agent_close_if_needed(node_id) {
            return false;
        }
        self.release_all_browser_input();
        let event = InputEvent::CloseNode { node: node_id };
        let Route::RemoveNode(node_id) = route(&self.runtime.focus.mode(), &event) else {
            return false;
        };
        if !self.remove_node_and_reflow(node_id, window) {
            return false;
        }
        self.interaction.drag = None;
        self.interaction.panning = false;
        self.interaction.last_screen = None;
        true
    }

    pub(crate) fn mark_node_viewed_if_needed(&mut self, node_id: NodeId) -> bool {
        mark_runtime_node_viewed_if_needed(&mut self.runtime, node_id)
    }

    pub(crate) fn visible_canvas_rect(&self, window: &mut Window) -> CanvasRect {
        let vp = self.runtime.document.viewport;
        let win = window.viewport_size();
        let top_left = vp.screen_to_canvas(Vec2::ZERO);
        let bottom_right =
            vp.screen_to_canvas(Vec2::new(f32::from(win.width), f32::from(win.height)));
        CanvasRect::new(
            top_left.x,
            top_left.y,
            bottom_right.x - top_left.x,
            bottom_right.y - top_left.y,
        )
    }

    pub(crate) fn layout_gap_canvas(&self) -> f32 {
        layout_gap_for_zoom(self.runtime.document.viewport.zoom)
    }

    fn apply_tiled_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        apply_workspace_reflow_for_area(&mut self.runtime, &mut self.motion, area, zoom);
    }

    pub(crate) fn reflow_tiled_nodes(&mut self, window: &mut Window) {
        self.master_layout.clear();
        self.apply_tiled_nodes(window);
    }

    pub(crate) fn apply_layout_for_current_mode(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        if self.master_layout.is_active()
            && reapply_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                area,
                zoom,
            )
        {
            return;
        }
        self.apply_tiled_nodes(window);
    }

    fn reflow_if_window_size_changed(&mut self, window: &mut Window) {
        let size = window.viewport_size();
        let current = (f32::from(size.width), f32::from(size.height));
        if self.last_reflow_window_size != Some(current) {
            self.last_reflow_window_size = Some(current);
            self.apply_layout_for_current_mode(window);
        }
    }

    fn on_toolbar_canvas_scroll(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction.canvas_scroll_enabled = !self.interaction.canvas_scroll_enabled;
        cx.notify();
    }

    pub(crate) fn remove_recent_project(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Err(error) = self.workspace_state.remove_recent_workspace(path) {
            tracing::warn!(%error, path = %path.display(), "failed to remove recent project");
        }
        self.recent_project_context_path = None;
        cx.notify();
    }
}

fn focused_cursor_animation_active(
    node_type: NodeType,
    node_run_state: Option<NodeRunState>,
    agent_task_state: Option<AgentTaskState>,
) -> bool {
    match node_type {
        NodeType::Term => matches!(node_run_state, Some(NodeRunState::Running)),
        NodeType::Agent => agent_chase_intent(agent_task_state),
        NodeType::Rect | NodeType::Preview | NodeType::Browser => false,
    }
}

impl Focusable for MultisoulApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MultisoulApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.voice_window_inset_window_visible = state::window_visible_for_voice_inset(window);
        self.sync_voice_window_inset_enabled();
        let mut root = div()
            .key_context("MultisoulApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_window))
            .on_action(cx.listener(Self::on_open_project))
            .on_action(cx.listener(Self::on_exit_focus))
            .on_action(cx.listener(Self::on_browser_command))
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
                    .on_action(cx.listener(Self::on_new_claude_agent))
                    .on_action(cx.listener(Self::on_new_codex_agent))
                    .on_action(cx.listener(Self::on_new_cursor_agent))
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

#[cfg(test)]
#[path = "../../tests/unit/app_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/app_cursor_animation_tests.rs"]
mod cursor_animation_tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "../../tests/unit/video_decoder_tests.rs"]
mod video_decoder_tests;
