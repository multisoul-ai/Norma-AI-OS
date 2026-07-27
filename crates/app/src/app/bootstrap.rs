use super::{keyboard::init_multisoul_key_bindings, AppScreen, MultisoulApp};
use crate::app::browser::BrowserStore;
use crate::app::frame_throttle::FrameThrottle;
use crate::canvas::interaction::InteractionState;
use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::NormaServer;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::agents::AgentDefinitionsState;
use crate::settings::orchestration::OrchestrationSettingsState;
use crate::settings::panel::{SettingsPanelState, SettingsViewportState};
use crate::settings::voice::VoiceSettingsInputs;
use crate::settings::wallpaper::WallpaperSettingsState;
use crate::settings::ThemeSettingsState;
use crate::shortcuts::ShortcutSettingsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use gpui::{Context, FocusHandle};
use runtime::Runtime;
use ui::Theme;

#[cfg(feature = "e2e-test-support")]
mod visual_preview;

#[derive(Clone, Copy)]
pub(super) enum BootstrapSettingsSource {
    Stored,
    #[cfg(any(test, feature = "e2e-test-support"))]
    InMemory,
}

impl MultisoulApp {
    pub(super) fn build(
        cx: &mut Context<Self>,
        runtime: Runtime,
        agent_hook_server: AgentHookServer,
        database_paths: storage::DatabasePaths,
        app_store: storage::AppStore,
        settings_source: BootstrapSettingsSource,
    ) -> Self {
        crate::network_crypto::install_network_crypto_provider();
        // 测试 / e2e 不经 `run_norma_os` 时也要装好组件库（幂等）。
        ui::init_components(cx);
        init_multisoul_key_bindings(cx);
        cx.on_release(|app, cx| {
            app.unregister_workspace_window_if_needed(cx);
        })
        .detach();
        Self::start_frame_loops(cx);
        let settings_store = app_store.settings_store();
        let wallpaper_settings = WallpaperSettingsState::load_from_store(settings_store.clone());
        let (theme_id, theme_settings, voice_settings, shortcuts) = match settings_source {
            BootstrapSettingsSource::Stored => {
                let (theme_id, theme_settings) =
                    ThemeSettingsState::load_from_store(settings_store.clone());
                let voice_settings = crate::settings::voice::VoiceSettingsState::load_from_store(
                    settings_store.clone(),
                );
                let shortcuts = ShortcutSettingsState::load_from_store(settings_store);
                (theme_id, theme_settings, voice_settings, shortcuts)
            }
            #[cfg(any(test, feature = "e2e-test-support"))]
            BootstrapSettingsSource::InMemory => {
                let (theme_id, theme_settings) = ThemeSettingsState::in_memory_default();
                (
                    theme_id,
                    theme_settings,
                    crate::settings::voice::VoiceSettingsState::in_memory_default(),
                    ShortcutSettingsState::default(),
                )
            }
        };
        let theme = Theme::for_id(theme_id);
        let focus_handle: FocusHandle = cx.focus_handle();
        let voice_settings_inputs = VoiceSettingsInputs::new(
            cx.weak_entity(),
            focus_handle.clone(),
            &voice_settings,
            theme,
            cx,
        );
        let workspace_state = WorkspaceState::load_from_storage(app_store.recent_store());
        crate::app::dialogue::configure_selected_tts_model(None);
        let mut app = MultisoulApp {
            database_paths,
            test_database_guard: None,
            runtime,
            theme_id,
            theme,
            theme_settings,
            interaction: InteractionState::default(),
            motion: MotionStore::from_env(),
            previews: PreviewStore::default(),
            browsers: BrowserStore::default(),
            browser_address_edit: None,
            master_layout: MasterLayoutState::default(),
            persistence: PersistenceScheduler::default(),
            settings_panel: SettingsPanelState::default(),
            settings_viewport: SettingsViewportState::default(),
            agent_definitions: AgentDefinitionsState::default(),
            orchestration_settings: OrchestrationSettingsState::default(),
            voice_settings,
            voice_settings_inputs,
            shortcuts,
            norma_server: NormaServer::start(),
            agent_hook_server,
            agent_announcements: super::agent_announcements::AgentAnnouncements::default(),
            agent_announcement_playback:
                super::agent_announcements::AgentAnnouncementPlayback::default(),
            screen: AppScreen::Launchpad,
            workspace_state,
            recent_project_context_path: None,
            voice: crate::app::voice::control::VoiceControlState::default(),
            dialogue: crate::app::dialogue::DialogueControlState::default(),
            tts_preview: crate::app::dialogue::TtsPreviewState::default(),
            pending_tts_activation: None,
            pending_minimax_preview_after_validation: None,
            tts_selection_generation: 0,
            tts_scan_generation: 0,
            asr_download_cancellations: std::collections::HashMap::new(),
            tts_download_cancellations: std::collections::HashMap::new(),
            voice_ready_notification_visible: false,
            presentation_notification: None,
            presentation_notification_generation: 0,
            agent_runtime: None,
            agent_task_coordinator: None,
            orchestration_authorizations: storage::OrchestrationAuthorizationCache::default(),
            pending_agent_submissions:
                super::agent_runtime::pending::PendingAgentSubmissions::default(),
            session_reply_queue: super::agent_runtime::AgentSessionReplyQueue::default(),
            conversation_tail_memory: super::agent_runtime::ConversationTailMemory::default(),
            conversation_tail_writer: None,
            pending_agent_close_confirmation: None,
            focus_handle,
            wallpaper_settings,
            wallpaper_previews: crate::settings::wallpaper::PreviewCache::default(),
            #[cfg(target_os = "macos")]
            video_wallpaper: None,
            last_reflow_window_size: None,
            terminal_cursor_phase: 0.0,
            terminal_render_cache:
                crate::cards::term::render_cache::terminal_render_cache_handle_from_env(),
            terminal_cursor_repaint: FrameThrottle::default(),
            #[cfg(feature = "e2e-test-support")]
            canvas_render_count_for_e2e: 0,
            window_active: true,
            voice_window_inset: crate::app::voice_window_inset::VoiceWindowInset::default(),
            voice_window_inset_window_visible: true,
        };
        #[cfg(feature = "e2e-test-support")]
        let visual_preview = visual_preview::configure_from_env(&mut app, cx);
        #[cfg(not(feature = "e2e-test-support"))]
        let visual_preview = false;
        app.refresh_asr_model_states(cx);
        if !visual_preview {
            app.refresh_tts_model_states(cx);
        }
        app.refresh_voice_microphone_requirement(cx);
        app.refresh_voice_provider_credentials(cx);
        app.validate_saved_voice_provider(cx);
        if matches!(settings_source, BootstrapSettingsSource::Stored) {
            app.persist_voice_settings_in_background(cx);
        }
        #[cfg(not(test))]
        if should_refresh_orchestration_integrations(visual_preview) {
            app.refresh_orchestration_integrations(cx);
        }
        app
    }
}

fn should_refresh_orchestration_integrations(visual_preview: bool) -> bool {
    !visual_preview
}

#[cfg(test)]
mod tests {
    use super::should_refresh_orchestration_integrations;

    /// 固定视觉数据保护：真实 App 的 canonical 预览不得被本机安装状态覆盖
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   preview modes = enabled(1) + disabled(1) = 2 cases
    ///   expected refreshes = normal startup(1) + visual preview(0) = 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 普通启动传入 `visual_preview=false` → 允许刷新真实 Provider 安装状态。
    ///   2. canonical 截图启动传入 `visual_preview=true` → 保留固定验收数据。
    ///
    /// 预期结果：
    ///   - 正断言：普通启动必须刷新，用户看到真实安装状态。
    ///   - 负断言：视觉预览不得刷新，避免覆盖 canonical Ready/Needs repair 状态。
    #[test]
    fn orchestration_refresh_preserves_canonical_visual_preview_data() {
        assert!(
            should_refresh_orchestration_integrations(false),
            "normal startup must refresh real orchestration integration state"
        );
        assert!(
            !should_refresh_orchestration_integrations(true),
            "canonical visual preview must not refresh over fixed acceptance data"
        );
    }
}
