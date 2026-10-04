use super::{keyboard::init_multisoul_key_bindings, AppScreen, MultisoulApp};
use crate::app::browser::BrowserStore;
use crate::app::feature_guide::FeatureGuideSettingsState;
use crate::app::frame_throttle::FrameThrottle;
use crate::canvas::interaction::InteractionState;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::agents::AgentDefinitionsState;
use crate::settings::orchestration::OrchestrationSettingsState;
use crate::settings::panel::{SettingsPanelState, SettingsViewportState};
use crate::settings::performance::PerformanceSettingsState;
use crate::settings::voice::VoiceSettingsInputs;
use crate::settings::wallpaper::WallpaperSettingsState;
use crate::settings::ThemeSettingsState;
use crate::shortcuts::ShortcutSettingsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use gpui::{AppContext as _, Context, FocusHandle};
use runtime::Runtime;
use ui::Theme;

mod stored;
#[cfg(feature = "e2e-test-support")]
mod visual_preview;
pub(crate) use stored::StoredBootstrap;

pub(super) enum BootstrapSettingsSource {
    Stored,
    Background(Box<StoredBootstrap>),
    #[cfg(any(test, feature = "e2e-test-support"))]
    InMemory,
}

pub(super) enum OrchestrationBootstrap {
    #[cfg(any(test, feature = "e2e-test-support"))]
    Skip,
    CurrentProcess,
    #[cfg(feature = "e2e-test-support")]
    Paths(Box<norma::orchestration::InstallPaths>),
}

impl MultisoulApp {
    pub(super) fn build(
        cx: &mut Context<Self>,
        runtime: Runtime,
        agent_hook_server: AgentHookServer,
        database_paths: storage::DatabasePaths,
        app_store: storage::AppStore,
        settings_source: BootstrapSettingsSource,
        orchestration_bootstrap: OrchestrationBootstrap,
    ) -> Self {
        crate::network_crypto::install_network_crypto_provider();
        crate::workspace::sessions::remember_database(database_paths.clone(), cx);
        let background = matches!(&settings_source, BootstrapSettingsSource::Background(_));
        let persist_voice = matches!(&settings_source, BootstrapSettingsSource::Stored);
        // 测试 / e2e 不经 `run_norma_os` 时也要装好组件库（幂等）。
        ui::init_components(cx);
        init_multisoul_key_bindings(cx);
        cx.on_release(|app, cx| {
            app.release_clone_task_if_owned(cx);
            app.unregister_workspace_window_if_needed(cx);
        })
        .detach();
        cx.on_app_quit(|app, cx| {
            app.stop_project(cx);
            async {}
        })
        .detach();
        let mobile = crate::mobile::controller::attach(app_store.clone(), cx);
        cx.observe(&mobile, |_, _, cx| cx.notify()).detach();
        let workspace_identity_store = app_store.workspace_identity_store();
        let orchestration_task_route_store = app_store.orchestration_task_route_store();
        crate::norma::endpoint::attach_task_store(orchestration_task_route_store.clone(), cx);
        let prepared = match settings_source {
            BootstrapSettingsSource::Background(prepared) => *prepared,
            BootstrapSettingsSource::Stored => StoredBootstrap::load(&app_store),
            #[cfg(any(test, feature = "e2e-test-support"))]
            BootstrapSettingsSource::InMemory => StoredBootstrap::in_memory(&app_store),
        };
        let wallpaper_settings = prepared.wallpaper;
        let workspace_state = prepared.workspace;
        let desktop_workspace = prepared.desktop;
        let (
            theme_id,
            theme_settings,
            performance_settings,
            voice_settings,
            shortcuts,
            feature_guide,
            feature_guide_settings,
        ) = (
            prepared.theme_id,
            prepared.theme,
            prepared.performance,
            prepared.voice,
            prepared.shortcuts,
            prepared.guide,
            prepared.guide_settings,
        );
        let (animation_wake_tx, animation_wake_rx) = async_channel::bounded(1);
        Self::start_frame_loops(animation_wake_rx, cx);
        let theme = Theme::for_id(theme_id);
        if !background {
            ui::sync_component_theme(theme_id, cx);
        }
        let focus_handle: FocusHandle = cx.focus_handle();
        let performance = crate::performance_monitor::register_window(cx);
        let feature_guide_primary_focus = cx.focus_handle().tab_index(1).tab_stop(true);
        let voice_settings_inputs = VoiceSettingsInputs::new(
            cx.weak_entity(),
            focus_handle.clone(),
            &voice_settings,
            theme,
            cx,
        );
        let (voice_window_inset_events, voice_window_inset_wake_rx) =
            crate::app::voice_window_inset::VoiceWindowInsetEventBus::new();
        let mut dialogue = crate::app::dialogue::DialogueControlState::default();
        dialogue.connect_playback_event_sink(
            voice_window_inset_events
                .sink(crate::app::voice_window_inset::TtsPlaybackSource::Assistant),
        );
        let mut tts_preview = crate::app::dialogue::TtsPreviewState::default();
        tts_preview.connect_playback_event_sink(
            voice_window_inset_events
                .sink(crate::app::voice_window_inset::TtsPlaybackSource::Preview),
        );
        let mut agent_announcement_playback =
            super::agent_announcements::AgentAnnouncementPlayback::default();
        agent_announcement_playback.connect_playback_event_sink(
            voice_window_inset_events
                .sink(crate::app::voice_window_inset::TtsPlaybackSource::AgentAnnouncement),
        );
        let voice_window_inset_view = cx.new({
            let events = voice_window_inset_events.clone();
            move |voice_cx| {
                crate::app::voice_window_inset::VoiceWindowInsetView::new(
                    events,
                    voice_window_inset_wake_rx,
                    voice_cx,
                )
            }
        });
        let canvas_surface = cx.new({
            let app = cx.weak_entity();
            let app_entity = cx.entity();
            move |surface_cx| {
                super::canvas_surface::CanvasSurfaceView::new(app, app_entity, surface_cx)
            }
        });
        let mut app = MultisoulApp {
            database_paths,
            mobile,
            test_database_guard: None,
            runtime,
            theme_id,
            theme,
            theme_settings,
            performance_settings,
            interaction: InteractionState::default(),
            motion: MotionStore::from_env(),
            previews: PreviewStore::default(),
            browsers: BrowserStore::default(),
            ego: super::ego::EgoController::default(),
            ego_previews: super::ego_preview::EgoPreviewStore::default(),
            browser_address_edit: None,
            command_dock: None,
            master_layout: MasterLayoutState::default(),
            layout_strategy: CanvasLayoutStrategy::default(),
            persistence: PersistenceScheduler::default(),
            settings_panel: SettingsPanelState::default(),
            norma_memory: None,
            norma_persona: None,
            norma_memory_cache: Default::default(),
            settings_viewport: SettingsViewportState::default(),
            feature_guide,
            feature_guide_settings,
            feature_guide_media: super::feature_guide::FeatureGuideMediaState::default(),
            feature_guide_primary_focus,
            #[cfg(feature = "e2e-test-support")]
            feature_guide_primary_focused_for_e2e: false,
            agent_definitions: AgentDefinitionsState::default(),
            orchestration_settings: OrchestrationSettingsState::default(),
            voice_settings,
            voice_settings_inputs,
            shortcuts,
            agent_hook_server,
            workspace_identity_store,
            workspace_id: None,
            workspace_route_id: None,
            agent_announcements: super::agent_announcements::AgentAnnouncements::default(),
            agent_announcement_playback,
            agent_announcement_audio_cache:
                super::agent_announcements::AgentAnnouncementAudioCache::default(),
            screen: AppScreen::Launchpad,
            workspace_state,
            desktop_workspace,
            clone_project: crate::workspace::clone_project::CloneProjectUiState::default(),
            recent_project_context_path: None,
            voice: crate::app::voice::control::VoiceControlState::default(),
            doubao: crate::app::doubao::DoubaoVoiceState::default(),
            dialogue,
            tts_preview,
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
            agent_queue_ui: super::agent_queue::AgentQueueUiState::default(),
            orchestration_authorizations: storage::OrchestrationAuthorizationCache::default(),
            pending_agent_submissions:
                super::agent_runtime::pending::PendingAgentSubmissions::default(),
            session_reply_queue: super::agent_runtime::AgentSessionReplyQueue::default(),
            pending_mobile_deliveries: std::collections::HashMap::new(),
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
            voice_caption_waveform_phase: 0.0,
            terminal_render_cache:
                crate::cards::term::render_cache::terminal_render_cache_handle_from_env(),
            performance,
            terminal_cursor_repaint: FrameThrottle::default(),
            #[cfg(feature = "e2e-test-support")]
            canvas_render_count_for_e2e: 0,
            #[cfg(feature = "e2e-test-support")]
            animation_frame_loop_paused_for_e2e: false,
            animation_wake_tx,
            animation_frame_clock: super::frame_loop::AnimationFrameClock::new(
                cx.background_executor().now(),
            ),
            animation_frame_pending: false,
            animation_frame_cadence: None,
            window_active: !background,
            project_window: None,
            background_project: background,
            desktop_host: None,
            desktop_switcher_subscription: None,
            background_restore: None,
            desktop_overview_scroll: std::collections::HashMap::new(),
            desktop_directory_scroll: gpui::ScrollHandle::new(),
            canvas_surface,
            working_snake_views: std::collections::HashMap::new(),
            voice_window_inset_events,
            voice_window_inset_view,
            voice_window_inset_window_visible: true,
        };
        #[cfg(feature = "e2e-test-support")]
        let visual_preview = visual_preview::configure_from_env(&mut app, cx);
        #[cfg(not(feature = "e2e-test-support"))]
        let visual_preview = false;
        if !background {
            app.refresh_desktop_previews(cx);
            app.refresh_asr_model_states(cx);
            if !visual_preview {
                app.refresh_tts_model_states(cx);
            }
            app.refresh_voice_microphone_requirement(cx);
            app.refresh_voice_provider_credentials(cx);
            app.validate_saved_voice_provider(cx);
            if persist_voice {
                app.persist_voice_settings_in_background(cx);
            }
        }
        #[cfg(test)]
        let _ = &orchestration_bootstrap;
        #[cfg(not(test))]
        {
            let shared =
                crate::settings::orchestration::health::register_window(cx.weak_entity(), cx);
            if let Some(shared) = shared {
                crate::settings::orchestration::health::apply_snapshot(&mut app, shared);
            }
            if !background
                && should_reconcile_orchestration_integrations(
                    visual_preview,
                    &orchestration_bootstrap,
                )
            {
                let install_paths = match orchestration_bootstrap {
                    #[cfg(any(test, feature = "e2e-test-support"))]
                    OrchestrationBootstrap::Skip => None,
                    OrchestrationBootstrap::CurrentProcess => None,
                    #[cfg(feature = "e2e-test-support")]
                    OrchestrationBootstrap::Paths(paths) => Some(*paths),
                };
                crate::settings::orchestration::health::refresh(
                    app.database_paths.app_db_path().to_path_buf(),
                    install_paths,
                    cx,
                );
            }
        }
        app
    }
}

fn should_reconcile_orchestration_integrations(
    visual_preview: bool,
    orchestration_bootstrap: &OrchestrationBootstrap,
) -> bool {
    if visual_preview {
        return false;
    }
    match orchestration_bootstrap {
        #[cfg(any(test, feature = "e2e-test-support"))]
        OrchestrationBootstrap::Skip => false,
        OrchestrationBootstrap::CurrentProcess => true,
        #[cfg(feature = "e2e-test-support")]
        OrchestrationBootstrap::Paths(paths) => {
            let _ = paths;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{should_reconcile_orchestration_integrations, OrchestrationBootstrap};

    /// 启动期编排升级：普通 App 自动 reconcile，canonical 预览不得触碰本机安装状态。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   startup modes = normal(1) + visual preview(1) + isolated test(1) = 3 cases
    ///   expected reconciles = normal startup(1) + visual preview(0) + isolated test(0) = 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 普通启动传入 `visual_preview=false` → 自动 reconcile 已授权 Provider。
    ///   2. canonical 截图启动传入 `visual_preview=true` → 跳过 reconcile，保留固定验收数据。
    ///   3. 隔离测试传入 `Skip` → 不得扫描或改写真实用户目录。
    ///
    /// 预期结果：
    ///   - 正断言：普通启动必须自动修复 App 升级造成的受管组件漂移。
    ///   - 负断言：视觉预览不得读写本机编排组件，避免污染 canonical 状态。
    ///   - 负断言：隔离测试不得访问真实用户目录中的编排安装。
    #[test]
    fn orchestration_startup_reconciles_real_installations_but_skips_visual_preview() {
        assert!(
            should_reconcile_orchestration_integrations(
                false,
                &OrchestrationBootstrap::CurrentProcess,
            ),
            "普通 App 启动必须自动 reconcile 已授权的真实编排安装"
        );
        assert!(
            !should_reconcile_orchestration_integrations(
                true,
                &OrchestrationBootstrap::CurrentProcess,
            ),
            "canonical 视觉预览不得读取或改写本机编排安装"
        );
        assert!(
            !should_reconcile_orchestration_integrations(false, &OrchestrationBootstrap::Skip),
            "隔离测试启动必须显式跳过真实用户目录中的编排安装"
        );
    }
}
