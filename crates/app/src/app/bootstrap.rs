use super::{AppScreen, MultisoulApp};
use crate::app::browser::BrowserStore;
use crate::app::frame_throttle::FrameThrottle;
use crate::canvas::interaction::InteractionState;
use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::NormaServer;
use crate::preview::PreviewStore;
use crate::settings::agent_hooks::AgentHookServer;
use crate::settings::agents::AgentDefinitionsState;
use crate::settings::panel::SettingsPanelState;
use crate::settings::voice::VoiceSettingsInputs;
use crate::settings::wallpaper::WallpaperSettingsState;
use crate::shortcuts::ShortcutSettingsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use gpui::{Context, FocusHandle};
use runtime::Runtime;
use ui::Theme;

impl MultisoulApp {
    pub(super) fn build(
        cx: &mut Context<Self>,
        runtime: Runtime,
        agent_hook_server: AgentHookServer,
    ) -> Self {
        // 测试 / e2e 不经 `run_norma_os` 时也要装好组件库（幂等）。
        ui::init_components(cx);
        cx.on_release(|app, cx| {
            app.unregister_workspace_window_if_needed(cx);
        })
        .detach();
        Self::start_frame_loops(cx);
        let wallpaper_settings = WallpaperSettingsState::load_default();
        let theme = Theme::dark();
        let focus_handle: FocusHandle = cx.focus_handle();
        let (voice_settings, voice_settings_inputs) =
            VoiceSettingsInputs::load_default(cx.weak_entity(), focus_handle.clone(), theme, cx);
        crate::app::dialogue::configure_selected_tts_model(None);
        let mut app = MultisoulApp {
            runtime,
            theme,
            interaction: InteractionState::default(),
            motion: MotionStore::from_env(),
            previews: PreviewStore::default(),
            browsers: BrowserStore::default(),
            browser_address_edit: None,
            master_layout: MasterLayoutState::default(),
            persistence: PersistenceScheduler::default(),
            settings_panel: SettingsPanelState::default(),
            agent_definitions: AgentDefinitionsState::default(),
            voice_settings,
            voice_settings_inputs,
            shortcuts: ShortcutSettingsState::load_default(),
            norma_server: NormaServer::start(),
            agent_hook_server,
            agent_announcements: super::agent_announcements::AgentAnnouncements::default(),
            agent_announcement_playback:
                super::agent_announcements::AgentAnnouncementPlayback::default(),
            screen: AppScreen::Launchpad,
            workspace_state: WorkspaceState::load_default(),
            voice: crate::app::voice::control::VoiceControlState::default(),
            dialogue: crate::app::dialogue::DialogueControlState::default(),
            tts_preview: crate::app::dialogue::TtsPreviewState::default(),
            tts_selection_generation: 0,
            tts_scan_generation: 0,
            tts_download_cancellations: std::collections::HashMap::new(),
            agent_runtime: None,
            pending_voice_commands: Vec::new(),
            focus_handle,
            wallpaper_settings,
            #[cfg(target_os = "macos")]
            video_wallpaper: None,
            last_reflow_window_size: None,
            terminal_cursor_phase: 0.0,
            terminal_cursor_repaint: FrameThrottle::default(),
            voice_window_inset: crate::app::voice_window_inset::VoiceWindowInset::default(),
            voice_window_inset_window_visible: true,
        };
        app.refresh_tts_model_states(cx);
        app
    }
}
