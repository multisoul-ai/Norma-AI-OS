use crate::app::browser::{BrowserAddressEdit, BrowserStore};
use crate::app::frame_throttle::FrameThrottle;
#[cfg(test)]
use crate::app::frame_throttle::CURSOR_REPAINT_INTERVAL;
use crate::canvas::interaction::InteractionState;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
#[cfg(test)]
use crate::canvas::master_layout::close_node_and_reflow_for_area;
use crate::canvas::master_layout::{term_grid_size_for_card, MasterLayoutState};
use crate::canvas::overlays::render_canvas_overlays;
use crate::canvas::spawn_focus;
use crate::canvas::view::render_canvas;
use crate::cards::snake::WorkingSnakeView;
use crate::cards::term::render_cache::TerminalRenderCacheHandle;
use crate::motion::MotionStore;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::agents::AgentDefinitionsState;
use crate::settings::orchestration::OrchestrationSettingsState;
use crate::settings::panel::{SettingsPanelState, SettingsViewportState};
use crate::settings::performance::PerformanceSettingsState;
use crate::settings::voice::{VoiceSettingsInputs, VoiceSettingsState};
use crate::settings::wallpaper::WallpaperSettingsState;
use crate::settings::ThemeSettingsState;
use crate::shortcuts::ShortcutSettingsState;
use crate::workspace::clone_project::CloneProjectUiState;
use crate::workspace::desktop::DesktopWorkspaceUiState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
#[cfg(test)]
use canvas::CanvasRect;
use canvas::{NodeId, NodeType, Vec2};
use gpui::{actions, div, prelude::*, px, Context, FocusHandle, Focusable, Window};
use runtime::{route, InputEvent, Route, Runtime};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
#[cfg(test)]
use std::time::Duration;
use ui::{Theme, ThemeId};
mod agent_actions;
mod agent_announcements;
mod agent_close;
mod agent_queue;
mod agent_runtime;
mod memory;
pub(crate) mod native_input;
pub(crate) use agent_runtime::execute_norma_adapter_command_with_source;
pub(crate) use agent_runtime::shared_tokio_runtime;
pub(crate) use agent_runtime::submit_workspace_turn;
pub(crate) use agent_runtime::{
    execute_high_level_agent_command, execute_orchestration_agent_command,
    execute_orchestration_agent_command_with_visual_origin, self_dispatch_forbidden_error,
    AgentIdentitySnapshot,
};
mod bootstrap;
pub(crate) use bootstrap::StoredBootstrap;
pub(crate) mod background_project;
#[cfg(test)]
pub(crate) use agent_runtime::execute_agent_card_command;
pub(crate) mod browser;
mod node_removal;
mod presentation;
pub(crate) use browser::{BrowserCommand, BrowserCommandKind};
mod canvas_surface;
pub(crate) mod cef_process;
mod chrome;
pub(crate) use chrome::{top_bar_theme, TOP_BAR_HEIGHT};
pub(crate) mod assistant_conversation;
mod clipboard_image;
pub(crate) mod command_dock;
mod cpu_benchmark;
mod cursor_animation;
#[cfg(test)]
use cursor_animation::focused_cursor_animation_active;
pub(crate) mod dialogue;
mod doubao;
mod e2e;
pub(crate) mod ego;
#[cfg(all(target_os = "macos", any(test, not(feature = "e2e-test-support"))))]
mod ego_page_capture;
#[cfg(target_os = "macos")]
mod ego_preview;
pub(crate) mod embedded_canvas;
#[cfg(all(target_os = "macos", feature = "l3-test-support"))]
pub use ego_preview::{run_ego_live_capture_probe, EgoLiveCaptureProbeReport};
pub(crate) mod file_picker;
mod frame_loop;
#[cfg(feature = "e2e-test-support")]
pub use frame_loop::runtime_frame_interval_for_e2e;
mod feature_guide;
mod frame_schedule;
mod frame_throttle;
pub(crate) use feature_guide::{
    settings_crossfade_opacities, FeatureGuidePresentation, FeatureGuideSettingsTransition,
    FeatureGuideTopic,
};
use feature_guide::{FeatureGuideMediaState, FeatureGuideSettingsState, FeatureGuideState};
mod ime;
pub(crate) use ime::should_defer_keystroke_to_ime;
mod ime_geometry;
mod keyboard;
mod keyboard_settings;
mod layout_actions;
pub(crate) mod layout_menu;
mod lifecycle;
mod render;
mod state;
#[cfg(target_os = "macos")]
pub(crate) mod video_decoder;
#[cfg(target_os = "macos")]
pub(crate) mod video_wallpaper;
mod voice;
pub(crate) use voice::caption::VoiceWaveformState;
#[cfg(not(feature = "e2e-test-support"))]
pub(crate) use voice::recognition::AsrRecordingSession;
mod optional_media;
mod voice_window_inset;
mod wallpaper_actions;
pub(crate) mod wallpaper_resolvers;
#[cfg(feature = "e2e-test-support")]
pub use e2e::dialogue_tts::*;
#[cfg(feature = "e2e-test-support")]
pub use e2e::CloneProjectE2eSnapshot;
#[cfg(feature = "e2e-test-support")]
pub use e2e::{
    install_norma_endpoint_for_e2e, mark_orchestration_health_ready_for_e2e,
    publish_canonical_orchestration_health_for_e2e, submit_norma_request_for_e2e,
};
pub use e2e::{
    AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot, ApiKeyDisplayState,
    CanvasLayoutE2eSnapshot, InitializationAnimationE2eSnapshot, SettingsE2eSnapshot,
    TtsProviderE2eSnapshot,
};
#[cfg(feature = "e2e-test-support")]
pub use e2e::{DispatchReceiptEffectE2eSnapshot, DispatchReceiptEffectItemE2eSnapshot};
#[cfg(feature = "e2e-test-support")]
pub use e2e::{
    EgoCardE2eSnapshot, FeatureGuideE2eSnapshot, OrchestrationProviderE2eSnapshot,
    OrchestrationSettingsE2eSnapshot, PresentationE2eSnapshot,
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
        NewGrokAgent,
        NewOpenCodeAgent,
        NewBrowser,
        NewEgo,
        CopyCardContent,
        OpenPreviewFile,
        OpenProject,
        CloneProject,
        OpenFeatureGuide,
        CheckForUpdates,
        ExitFocus,
        ReflowLayout,
        ToggleMasterLayout,
        ToggleSettings,
        QuitNormaOs
    ]
);

pub struct MultisoulApp {
    pub(crate) database_paths: storage::DatabasePaths,
    pub(crate) test_database_guard: Option<Arc<crate::database::TestDatabaseGuard>>,
    pub runtime: Runtime,
    pub theme_id: ThemeId,
    pub theme: Theme,
    pub(crate) theme_settings: ThemeSettingsState,
    pub(crate) performance_settings: PerformanceSettingsState,
    pub interaction: InteractionState,
    pub(crate) motion: MotionStore,
    pub(crate) previews: PreviewStore,
    pub(crate) browsers: BrowserStore,
    pub(crate) ego: ego::EgoController,
    pub(crate) ego_previews: ego_preview::EgoPreviewStore,
    pub(crate) browser_address_edit: Option<BrowserAddressEdit>,
    pub(crate) command_dock: Option<command_dock::CommandDockState>,
    pub(crate) assistant_conversation: assistant_conversation::AssistantConversation,
    pub(crate) master_layout: MasterLayoutState,
    pub(crate) layout_strategy: CanvasLayoutStrategy,
    pub(crate) persistence: PersistenceScheduler,
    pub(crate) settings_panel: SettingsPanelState,
    pub(crate) settings_viewport: SettingsViewportState,
    pub(crate) feature_guide: FeatureGuideState,
    pub(crate) feature_guide_settings: FeatureGuideSettingsState,
    pub(crate) feature_guide_media: FeatureGuideMediaState,
    pub(crate) feature_guide_primary_focus: FocusHandle,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) feature_guide_primary_focused_for_e2e: bool,
    pub(crate) agent_definitions: AgentDefinitionsState,
    pub(crate) orchestration_settings: OrchestrationSettingsState,
    pub(crate) mobile: gpui::Entity<crate::mobile::controller::MobileController>,
    pub(crate) norma_memory: Option<gpui::Entity<crate::settings::norma::MemoryView>>,
    pub(crate) norma_persona: Option<gpui::Entity<crate::settings::norma::persona::PersonaView>>,
    pub(crate) norma_memory_cache: std::collections::HashMap<
        Option<std::path::PathBuf>,
        gpui::Entity<crate::settings::norma::MemoryView>,
    >,
    pub(crate) voice_settings: VoiceSettingsState,
    pub(crate) voice_settings_inputs: VoiceSettingsInputs,
    pub(crate) shortcuts: ShortcutSettingsState,
    pub(crate) agent_hook_server: AgentHookServer,
    pub(crate) workspace_identity_store: storage::WorkspaceIdentityStore,
    pub(crate) workspace_id: Option<String>,
    pub(crate) workspace_route_id: Option<String>,
    agent_announcements: agent_announcements::AgentAnnouncements,
    agent_announcement_playback: agent_announcements::AgentAnnouncementPlayback,
    agent_announcement_audio_cache: agent_announcements::AgentAnnouncementAudioCache,
    pub(crate) screen: AppScreen,
    pub(crate) workspace_state: WorkspaceState,
    pub(crate) desktop_workspace: DesktopWorkspaceUiState,
    pub(crate) clone_project: CloneProjectUiState,
    pub(crate) recent_project_context_path: Option<PathBuf>,
    pub(crate) voice: crate::app::voice::control::VoiceControlState,
    pub(crate) doubao: doubao::DoubaoVoiceState,
    pub(crate) dialogue: crate::app::dialogue::DialogueControlState,
    pub(crate) tts_preview: crate::app::dialogue::TtsPreviewState,
    pub(crate) pending_tts_activation: Option<crate::app::dialogue::PendingTtsActivation>,
    pub(crate) pending_minimax_preview_after_validation: Option<u64>,
    pub(crate) tts_selection_generation: u64,
    pub(crate) tts_scan_generation: u64,
    pub(crate) asr_download_cancellations: HashMap<asr::AsrModelId, Arc<AtomicBool>>,
    pub(crate) tts_download_cancellations: HashMap<tts::TtsModelId, Arc<AtomicBool>>,
    pub(crate) voice_ready_notification_visible: bool,
    pub(crate) presentation_notification: Option<presentation::PresentationNotification>,
    pub(crate) presentation_notification_generation: u64,
    pub(crate) agent_runtime: Option<agent_runtime::ProjectAgentRuntime>,
    pub(crate) agent_task_coordinator: Option<agent::AgentTaskCoordinator>,
    pub(crate) agent_queue_ui: agent_queue::AgentQueueUiState,
    pub(crate) orchestration_authorizations: storage::OrchestrationAuthorizationCache,
    pub(crate) pending_agent_submissions: agent_runtime::pending::PendingAgentSubmissions,
    pub(crate) session_reply_queue: agent_runtime::AgentSessionReplyQueue,
    pub(crate) pending_mobile_deliveries: HashMap<NodeId, storage::NewMobileCommand>,
    pub(crate) conversation_tail_memory: agent_runtime::ConversationTailMemory,
    pub(crate) conversation_tail_writer: Option<agent_runtime::ConversationTailWriter>,
    pub(crate) pending_agent_close_confirmation: Option<NodeId>,
    pub focus_handle: FocusHandle,
    pub(crate) wallpaper_settings: WallpaperSettingsState,
    pub(crate) wallpaper_previews: crate::settings::wallpaper::PreviewCache,
    #[cfg(target_os = "macos")]
    pub(crate) video_wallpaper: Option<video_wallpaper::VideoWallpaperState>,
    pub(crate) last_reflow_window_size: Option<(f32, f32)>,
    pub(crate) desktop_embedded_canvas_size: Option<Vec2>,
    pub(crate) desktop_embedded_reflow_size: Option<Vec2>,
    pub(crate) desktop_embedded_layout_origin: Option<Vec2>,
    pub(crate) desktop_presentation_bounds: HashMap<NodeId, canvas::CanvasRect>,
    pub(crate) terminal_cursor_phase: f32,
    pub(crate) voice_caption_waveform_phase: f32,
    pub(crate) terminal_render_cache: TerminalRenderCacheHandle,
    pub(crate) performance: crate::performance_monitor::WindowPerformanceHandle,
    terminal_cursor_repaint: FrameThrottle,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) canvas_render_count_for_e2e: usize,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) animation_frame_loop_paused_for_e2e: bool,
    animation_wake_tx: async_channel::Sender<()>,
    animation_frame_clock: frame_loop::AnimationFrameClock,
    animation_frame_pending: bool,
    animation_frame_cadence: Option<std::time::Duration>,
    pub(crate) window_active: bool,
    pub(crate) project_window: Option<gpui::AnyWindowHandle>,
    pub(crate) background_project: bool,
    pub(crate) voice_runtime_checked: bool,
    pub(crate) desktop_host: Option<gpui::WeakEntity<MultisoulApp>>,
    pub(crate) desktop_switcher_subscription: Option<gpui::Subscription>,
    pub(crate) background_restore: Option<crate::workspace::flow::PendingLiveSessionRestore>,
    pub(crate) desktop_overview_scroll: HashMap<String, gpui::ScrollHandle>,
    pub(crate) desktop_directory_scroll: gpui::ScrollHandle,
    canvas_surface: gpui::Entity<canvas_surface::CanvasSurfaceView>,
    pub(crate) working_snake_views: HashMap<NodeId, gpui::Entity<WorkingSnakeView>>,
    voice_window_inset_events: voice_window_inset::VoiceWindowInsetEventBus,
    voice_window_inset_view: gpui::Entity<voice_window_inset::VoiceWindowInsetView>,
    voice_window_inset_window_visible: bool,
}
impl MultisoulApp {
    pub(crate) fn new_with_database_context(
        cx: &mut Context<Self>,
        database_paths: storage::DatabasePaths,
        app_store: storage::AppStore,
    ) -> Self {
        let agent_hook_server = AgentHookServer::start(cx);
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        Self::build(
            cx,
            runtime,
            agent_hook_server,
            database_paths,
            app_store,
            bootstrap::BootstrapSettingsSource::Stored,
            bootstrap::OrchestrationBootstrap::CurrentProcess,
        )
    }

    fn on_new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("chrome.open_terminals"));
            cx.notify();
            return;
        }

        let bounds = self.next_tiled_node_bounds(window);
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
                self.apply_layout_for_new_node(node_id, window);
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
        let home = crate::cards::browser::normalize_browser_url("");
        if self.route_project_content_to_desktop(
            storage::DesktopContentKind::Browser,
            home.clone(),
            "浏览器".to_string(),
            cx,
        ) {
            return;
        }
        if matches!(self.screen, AppScreen::DesktopWorkspace) {
            self.open_desktop_content_tab(
                storage::DesktopContentKind::Browser,
                home,
                "浏览器".to_string(),
                self.desktop_workspace.expanded_area_id.clone(),
                cx,
            );
            return;
        }
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some(ui::i18n::LocalizedText::new("chrome.open_browsers"));
            cx.notify();
            return;
        }

        let bounds = self.next_tiled_node_bounds(window);
        let node_id = self.runtime.spawn_browser(
            crate::cards::browser::normalize_browser_url(""),
            None,
            bounds,
        );
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.motion.spawn_node(node_id, bounds);
        self.apply_layout_for_new_node(node_id, window);
        self.mark_canvas_dirty();
        cx.notify();
    }

    fn on_new_ego(&mut self, _: &NewEgo, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some(ui::i18n::LocalizedText::new("chrome.open_ego"));
            cx.notify();
            return;
        }

        let node_id = if let Some(node_id) = self.runtime.ego_node_id() {
            node_id
        } else {
            let bounds = self.next_tiled_node_bounds(window);
            let node_id = self.runtime.spawn_ego_browser(None, bounds);
            self.motion.spawn_node(node_id, bounds);
            self.apply_layout_for_new_node(node_id, window);
            self.mark_canvas_dirty();
            node_id
        };
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.ensure_ego_task_space(node_id, cx);
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

    fn on_clone_project(&mut self, _: &CloneProject, window: &mut Window, cx: &mut Context<Self>) {
        self.open_clone_project_dialog(window, cx);
    }

    fn on_new_window(&mut self, _: &NewWindow, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_new_launchpad_window(cx);
    }

    fn on_open_preview_file(
        &mut self,
        _: &OpenPreviewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_create_preview() {
            self.workspace_state.error = Some(ui::i18n::LocalizedText::new("chrome.open_previews"));
            cx.notify();
            return;
        }
        self.prompt_preview_file(window, cx);
    }

    pub(crate) fn close_node_through_route(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
    ) -> bool {
        if self.previews.has_pending_edits(node_id) {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("preview.close_unsaved"));
            return false;
        }
        if self.runtime.ego_task_state(node_id).is_some()
            && !self.runtime.can_close_ego_node(node_id)
        {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("chrome.ego_task_active"));
            return false;
        }
        if !self.confirm_agent_close_if_needed(node_id, true) {
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
}
mod focusable;
#[cfg(test)]
mod test_modules;
