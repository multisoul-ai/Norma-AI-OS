//! Stable, primitive e2e projections that do not expose private settings models.

use super::{AppScreen, MultisoulApp};
use crate::settings::{agent_hooks::AgentHookServer, ThemeSettingsState};
use crate::shortcuts::{ShortcutCommandId, ShortcutSettingsState};
use gpui::{App, Context};
use runtime::Runtime;
#[cfg(feature = "e2e-test-support")]
use std::path::PathBuf;
use std::time::Duration;
use voice_sherpa::SpeechError;

#[cfg(feature = "e2e-test-support")]
mod agent_announcements;
mod agents;
#[cfg(feature = "e2e-test-support")]
mod browser;
mod recent_projects;
mod shortcuts;
mod tts;
mod voice;
#[cfg(feature = "e2e-test-support")]
mod voice_setup;
pub use agents::{AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot};
use shortcuts::shortcut_command_name;
pub use tts::TtsProviderE2eSnapshot;
use tts::{snapshot_tts_models, snapshot_tts_provider, TtsModelE2eSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiKeyDisplayState {
    NotRendered,
    NotConfigured,
    Configured,
    Revealed,
    MaskedEditing,
    Unsafe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsE2eSnapshot {
    pub settings_open: bool,
    pub active_section: &'static str,
    pub active_theme_id: &'static str,
    pub theme_is_dark: bool,
    pub theme_canvas_from: u32,
    pub theme_accent: u32,
    pub recording_command: Option<&'static str>,
    pub new_terminal_shortcut: Option<String>,
    pub new_terminal_modified: bool,
    pub status_message: Option<String>,
    pub agent_provider: &'static str,
    pub agent_base_url: String,
    pub agent_model: String,
    pub agent_api_key_configured: bool,
    pub agent_base_url_input_surface_size: Option<(u32, u32)>,
    pub agent_model_input_surface_size: Option<(u32, u32)>,
    pub agent_base_url_rendered_text: Option<String>,
    pub agent_model_rendered_text: Option<String>,
    pub agent_api_key_display_state: ApiKeyDisplayState,
    pub voice_editing_field: Option<&'static str>,
    pub voice_interaction_mode: &'static str,
    pub dictation_trigger: &'static str,
    pub input_device_id: Option<String>,
    pub trace_enabled: bool,
    pub agent_task_announcements_enabled: bool,
    pub voice_window_inset_enabled: bool,
    pub selected_asr_model_id: Option<String>,
    pub asr_models: Vec<AsrModelE2eSnapshot>,
    pub selected_tts_model_id: Option<String>,
    pub selected_tts_voice_id: Option<String>,
    pub runtime_tts_provider_id: Option<String>,
    pub runtime_tts_model_id: Option<String>,
    pub runtime_tts_voice_id: Option<String>,
    pub tts_provider: TtsProviderE2eSnapshot,
    pub zipvoice_reference_ready: bool,
    pub zipvoice_reference_importing: bool,
    pub zipvoice_reference_draft_path: String,
    pub zipvoice_reference_draft_transcript: String,
    pub tts_models: Vec<TtsModelE2eSnapshot>,
    pub tts_preview_active: bool,
    pub tts_configuration_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsrModelE2eSnapshot {
    pub id: String,
    pub display_name: String,
    pub languages: Vec<String>,
    pub streaming_partials: bool,
    pub recommended: bool,
    pub installation: &'static str,
    pub selected: bool,
    pub action: &'static str,
    pub deletable: bool,
}

/// 初始化动画的稳定 e2e 投影：不暴露 app 私有 MotionStore 或帧资源实现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitializationAnimationE2eSnapshot {
    pub kind: &'static str,
    pub asset: &'static str,
    pub frame_index: usize,
    pub neutral_visible_cells: usize,
    pub accent_visible_cells: usize,
    pub overlay_opacity_milli: u16,
    pub content_opacity_milli: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoiceWindowInsetE2eSnapshot {
    pub active: bool,
    pub opacity_milli: u16,
    pub inset_px_milli: u16,
    pub corner_seam_guard_alpha_milli: u16,
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn set_voice_model_root_for_e2e(&mut self, model_root: PathBuf, cx: &mut Context<Self>) {
        self.voice_settings.reset_model_root_for_e2e(model_root);
        self.refresh_tts_model_states(cx);
        cx.notify();
    }

    /// 聚焦卡片当前是否应推进 terminal cursor 的运行中动画。Agent 卡片会同时考虑 PTY 生命周期
    /// 与 Agent task 状态，普通 Terminal 则只考虑 PTY 生命周期。
    pub fn focused_terminal_cursor_animation_active_e2e(&self) -> bool {
        self.focused_terminal_cursor_animation_active()
    }

    pub fn new_with_pty_factory(cx: &mut Context<Self>, factory: runtime::PtyFactory) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_pty_factory(factory);
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        let mut app = Self::build(cx, runtime, agent_hook_server);
        app.browsers.disable_native_views();
        let (theme_id, theme_settings) = ThemeSettingsState::in_memory_default();
        app.configure_theme_settings(theme_id, theme_settings, cx);
        app.shortcuts = ShortcutSettingsState::default();
        app.voice_settings = crate::settings::voice::VoiceSettingsState::in_memory_default();
        #[cfg(feature = "e2e-test-support")]
        app.voice_settings.assume_legacy_ready_for_e2e();
        app.voice_settings_inputs
            .sync_from_state(&app.voice_settings, cx);
        app
    }

    pub fn disable_agent_hook_for_test(&mut self) {
        self.agent_hook_server.disable_for_test();
    }

    pub fn new_with_pty_factory_and_voice_settings_store(
        cx: &mut Context<Self>,
        factory: runtime::PtyFactory,
        store: storage::SettingsStore,
    ) -> Self {
        let mut app = Self::new_with_pty_factory(cx, factory);
        let (theme_id, theme_settings) = ThemeSettingsState::load_from_store(store.clone());
        app.configure_theme_settings(theme_id, theme_settings, cx);
        app.voice_settings = crate::settings::voice::VoiceSettingsState::load_from_store(store);
        #[cfg(feature = "e2e-test-support")]
        app.voice_settings.assume_legacy_ready_for_e2e();
        app.voice_settings_inputs
            .sync_from_state(&app.voice_settings, cx);
        app
    }

    pub fn load_voice_provider_settings_for_e2e(
        store: storage::SettingsStore,
    ) -> (String, String, bool) {
        let state = crate::settings::voice::VoiceSettingsState::load_from_store(store);
        let profile = state.values().active_provider_profile();
        (
            profile.base_url().to_string(),
            profile.model().to_string(),
            profile.credential_configured(),
        )
    }

    pub fn screen(&self) -> AppScreen {
        self.screen
    }

    /// Launchpad 的固定背景资源；Canvas 的 Wallpaper 设置不会改变此值。
    pub fn launchpad_video_asset_e2e(&self) -> &'static str {
        crate::workspace::launchpad::launchpad_default_video_asset()
    }

    pub fn initialization_e2e_snapshot(
        &self,
        node_id: canvas::NodeId,
    ) -> Option<InitializationAnimationE2eSnapshot> {
        self.motion
            .initialization_presentation(node_id)
            .map(|presentation| {
                let (neutral_visible_cells, accent_visible_cells) =
                    crate::cards::intro::intro_visible_cell_counts(
                        presentation.kind,
                        presentation.frame_index,
                    );
                InitializationAnimationE2eSnapshot {
                    kind: match presentation.kind {
                        crate::motion::InitializationAnimationKind::Terminal => "terminal",
                        crate::motion::InitializationAnimationKind::Codex => "codex",
                        crate::motion::InitializationAnimationKind::Claude => "claude",
                        crate::motion::InitializationAnimationKind::Cursor => "cursor",
                        crate::motion::InitializationAnimationKind::Grok => "grok",
                        crate::motion::InitializationAnimationKind::OpenCode => "opencode",
                    },
                    asset: crate::cards::intro::intro_asset_name(presentation.kind),
                    frame_index: presentation.frame_index,
                    neutral_visible_cells,
                    accent_visible_cells,
                    overlay_opacity_milli: opacity_to_milli(presentation.overlay_opacity),
                    content_opacity_milli: opacity_to_milli(presentation.content_opacity),
                }
            })
    }

    /// 以精确逻辑时长推进 app 动效。真实窗口由 12ms 帧循环驱动；测试必须能直接验证 31ms 初始化帧和 80ms
    /// 用户中断边界，不能让帧循环量化误差改变产品时间线。
    pub fn advance_animations_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        let motion_changed = self.motion.tick(delta);
        let cursor_changed = self.advance_terminal_cursor_phase(delta);
        let voice_window_inset_changed = self.voice_window_inset.tick(delta);
        if motion_changed || cursor_changed || voice_window_inset_changed {
            cx.notify();
        }
    }

    pub fn begin_assistant_playback_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_window_inset.begin_playback();
        cx.notify();
    }

    /// 注入一块已成功排入播放时间线的助手 PCM，供 headless e2e 验证可见能量反馈。
    pub fn queue_assistant_pcm_for_e2e(
        &mut self,
        chunk: audio::PcmAudioChunk,
        cx: &mut Context<Self>,
    ) {
        self.voice_window_inset.queue_pcm_energy(&chunk);
        cx.notify();
    }

    pub fn finish_assistant_playback_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_window_inset.finish_playback();
        cx.notify();
    }

    pub fn voice_window_inset_e2e_snapshot(&self) -> VoiceWindowInsetE2eSnapshot {
        match self.voice_window_inset.visual() {
            Some(visual) => VoiceWindowInsetE2eSnapshot {
                active: true,
                opacity_milli: opacity_to_milli(visual.opacity),
                inset_px_milli: (visual.inset_px * 1000.0).round() as u16,
                corner_seam_guard_alpha_milli: opacity_to_milli(
                    self.voice_window_inset.corner_seam_guard_alpha(),
                ),
            },
            None => VoiceWindowInsetE2eSnapshot {
                active: false,
                opacity_milli: 0,
                inset_px_milli: 0,
                corner_seam_guard_alpha_milli: 0,
            },
        }
    }

    pub fn enter_canvas_for_test(&mut self, root: &std::path::Path, cx: &mut Context<Self>) {
        let project_store = storage::ProjectStore::for_workspace(root)
            .expect("e2e workspace root should initialize project storage");
        project_store
            .migrate()
            .expect("e2e project storage should migrate");
        self.agent_definitions
            .bind_project_store(project_store.agent_definition_store());
        self.persistence.set_project_store(project_store);
        self.runtime.set_workspace_root(root.to_path_buf());
        self.screen = AppScreen::Canvas;
        self.last_reflow_window_size = None;
        cx.notify();
    }

    pub fn show_workspace_error_for_e2e(
        &mut self,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.workspace_state.error = Some(message.into());
        cx.notify();
    }

    pub fn workspace_error_for_e2e(&self) -> Option<String> {
        self.workspace_state.error.clone()
    }

    /// 注入 TCC 拒绝错误供 L1 验证应用内恢复 UI；系统设置属于 L3，不能由 headless GPUI 修改。
    pub fn show_microphone_permission_denied_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice.set_speech_error(&SpeechError::MicrophoneDenied);
        cx.notify();
    }

    /// 注入稳定的麦克风列表，避免 headless e2e 依赖当前测试机硬件枚举结果。
    pub fn set_voice_input_devices_for_e2e(
        &mut self,
        devices: Vec<(String, String, bool)>,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .refresh_input_devices_from_result(Ok(devices
                .into_iter()
                .map(|(id, name, is_default)| voice_sherpa::AudioInputDevice {
                    id,
                    name,
                    is_default,
                })
                .collect()));
        cx.notify();
    }

    pub fn settings_e2e_snapshot(&self, cx: &App) -> SettingsE2eSnapshot {
        let new_terminal = self
            .shortcuts
            .effective
            .iter()
            .find(|entry| entry.command == ShortcutCommandId::NewTerminal);
        let api_key_text = self
            .voice_settings_inputs
            .rendered_text(crate::settings::voice::VoiceSettingsField::ApiKey, cx);
        let api_key_profile = self.voice_settings.values().active_provider_profile();
        let runtime_tts = crate::app::dialogue::selected_tts_model();
        SettingsE2eSnapshot {
            settings_open: self.settings_panel.open,
            active_section: self.settings_panel.active_section.title(),
            active_theme_id: self.theme_id.as_str(),
            theme_is_dark: self.theme_id.is_dark(),
            theme_canvas_from: self.theme.canvas_from,
            theme_accent: self.theme.accent,
            recording_command: self.shortcuts.recording.map(shortcut_command_name),
            new_terminal_shortcut: new_terminal.and_then(|entry| entry.current_keystroke.clone()),
            new_terminal_modified: new_terminal.map(|entry| entry.modified).unwrap_or(false),
            status_message: self.shortcuts.status.message.clone(),
            agent_provider: self.voice_settings.values().agent_provider().as_str(),
            agent_base_url: self
                .voice_settings
                .values()
                .active_provider_profile()
                .base_url()
                .to_string(),
            agent_model: self.voice_settings.active_agent_model().to_string(),
            agent_api_key_configured: self
                .voice_settings
                .values()
                .active_provider_profile()
                .credential_configured(),
            agent_base_url_input_surface_size: self
                .voice_settings_inputs
                .input_surface_size(crate::settings::voice::VoiceSettingsField::BaseUrl, cx),
            agent_model_input_surface_size: self
                .voice_settings_inputs
                .input_surface_size(crate::settings::voice::VoiceSettingsField::Model, cx),
            agent_base_url_rendered_text: self
                .voice_settings_inputs
                .rendered_text(crate::settings::voice::VoiceSettingsField::BaseUrl, cx),
            agent_model_rendered_text: self
                .voice_settings_inputs
                .rendered_text(crate::settings::voice::VoiceSettingsField::Model, cx),
            agent_api_key_display_state: classify_api_key_display(
                api_key_text.as_deref(),
                self.voice_settings_inputs.api_key_revealed(cx),
                api_key_profile.api_key(),
            ),
            voice_editing_field: self
                .voice_settings
                .editing_field()
                .map(crate::settings::voice::VoiceSettingsField::stable_name),
            voice_interaction_mode: self.voice_settings.values().interaction_mode().as_str(),
            dictation_trigger: self.voice_settings.values().dictation_trigger().as_str(),
            input_device_id: self
                .voice_settings
                .values()
                .input_device_id()
                .map(ToOwned::to_owned),
            trace_enabled: self.voice_settings.values().trace_enabled(),
            agent_task_announcements_enabled: self
                .voice_settings
                .values()
                .agent_task_announcements_enabled(),
            voice_window_inset_enabled: self.voice_settings.values().voice_window_inset_enabled(),
            selected_asr_model_id: self
                .voice_settings
                .values()
                .asr_model_id()
                .map(|id| id.as_str().to_string()),
            asr_models: self
                .voice_settings
                .model_manager()
                .entries()
                .iter()
                .map(|entry| {
                    let descriptor = entry.descriptor();
                    let selected =
                        self.voice_settings.values().asr_model_id() == Some(descriptor.id());
                    AsrModelE2eSnapshot {
                        id: descriptor.id().as_str().to_string(),
                        display_name: descriptor.display_name().to_string(),
                        languages: descriptor.capabilities().languages().to_vec(),
                        streaming_partials: descriptor.capabilities().streaming_partials(),
                        recommended: descriptor.recommended(),
                        installation: match entry.installation() {
                            crate::settings::voice::AsrModelInstallState::NotInstalled => {
                                "not_installed"
                            }
                            crate::settings::voice::AsrModelInstallState::Installing { .. } => {
                                "installing"
                            }
                            crate::settings::voice::AsrModelInstallState::Ready => "ready",
                            crate::settings::voice::AsrModelInstallState::Failed(_) => "failed",
                        },
                        selected,
                        action: match entry.installation() {
                            crate::settings::voice::AsrModelInstallState::NotInstalled => "install",
                            crate::settings::voice::AsrModelInstallState::Installing { .. } => {
                                "progress"
                            }
                            crate::settings::voice::AsrModelInstallState::Ready if selected => {
                                "selected"
                            }
                            crate::settings::voice::AsrModelInstallState::Ready => "select",
                            crate::settings::voice::AsrModelInstallState::Failed(_) => "retry",
                        },
                        deletable: matches!(
                            entry.installation(),
                            crate::settings::voice::AsrModelInstallState::Ready
                        ) && !selected,
                    }
                })
                .collect(),
            selected_tts_model_id: self
                .voice_settings
                .values()
                .tts_model_id()
                .map(|id| id.as_str().to_string()),
            selected_tts_voice_id: self
                .voice_settings
                .values()
                .tts_voice_id()
                .map(|id| id.as_str().to_string()),
            runtime_tts_provider_id: runtime_tts
                .as_ref()
                .map(|selected| selected.provider_id.as_str().to_string()),
            runtime_tts_model_id: runtime_tts
                .as_ref()
                .map(|selected| selected.model_id.as_str().to_string()),
            runtime_tts_voice_id: runtime_tts
                .as_ref()
                .map(|selected| selected.voice_id.as_str().to_string()),
            tts_provider: snapshot_tts_provider(&self.voice_settings),
            zipvoice_reference_ready: self.voice_settings.zipvoice_reference_ready(),
            zipvoice_reference_importing: self.voice_settings.zipvoice_reference_importing(),
            zipvoice_reference_draft_path: self
                .voice_settings
                .zipvoice_reference_draft_path()
                .to_string(),
            zipvoice_reference_draft_transcript: self
                .voice_settings
                .zipvoice_reference_draft_transcript()
                .to_string(),
            tts_models: snapshot_tts_models(&self.voice_settings),
            tts_preview_active: self.tts_preview.is_active(),
            tts_configuration_generation: self.dialogue.tts_configuration_generation(),
        }
    }
}

fn opacity_to_milli(opacity: f32) -> u16 {
    (opacity.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn classify_api_key_display(
    text: Option<&str>,
    revealed: bool,
    expected_secret: Option<&str>,
) -> ApiKeyDisplayState {
    match text {
        None => ApiKeyDisplayState::NotRendered,
        Some(value) if revealed && expected_secret == Some(value) => ApiKeyDisplayState::Revealed,
        Some(_) if revealed => ApiKeyDisplayState::Unsafe,
        Some("Not configured") => ApiKeyDisplayState::NotConfigured,
        Some("*********") => ApiKeyDisplayState::Configured,
        Some(value) if !value.is_empty() && value.chars().all(|character| character == '•') => {
            ApiKeyDisplayState::MaskedEditing
        }
        Some(_) => ApiKeyDisplayState::Unsafe,
    }
}
