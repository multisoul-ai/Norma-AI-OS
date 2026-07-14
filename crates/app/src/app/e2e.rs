//! Public e2e-facing app snapshots.
//!
//! These accessors keep external e2e tests on stable, primitive state without
//! exposing the full private settings model.

use super::{AppScreen, MultisoulApp};
use crate::settings::agent_hooks::AgentHookServer;
use crate::shortcuts::ShortcutCommandId;
use crate::shortcuts::ShortcutSettingsState;
use gpui::{App, Context};
use runtime::Runtime;
#[cfg(feature = "e2e-test-support")]
use std::path::PathBuf;
use std::time::Duration;
use voice_sherpa::SpeechError;

#[cfg(feature = "e2e-test-support")]
mod agent_announcements;
mod agents;
mod tts;
pub use agents::{AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot};
use tts::{snapshot_tts_models, TtsModelE2eSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiKeyDisplayState {
    NotRendered,
    NotConfigured,
    Configured,
    MaskedEditing,
    Unsafe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsE2eSnapshot {
    pub settings_open: bool,
    pub active_section: &'static str,
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

/// 语音入口的稳定 e2e 投影：覆盖用户可见状态，不暴露录音或对话实现对象。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceE2eSnapshot {
    pub phase: &'static str,
    pub dialogue_phase: &'static str,
    pub label: String,
    pub microphone_recovery_available: bool,
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn set_voice_model_root_for_e2e(&mut self, model_root: PathBuf, cx: &mut Context<Self>) {
        self.voice_settings.reset_model_root_for_e2e(model_root);
        self.refresh_tts_model_states(cx);
        cx.notify();
    }

    /// 聚焦卡片当前是否应推进 terminal cursor 的运行中动画。
    ///
    /// 这是面向 headless e2e 的稳定投影：Agent 卡片会同时考虑 PTY 生命周期
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
        app.shortcuts = ShortcutSettingsState::default();
        app.voice_settings = crate::settings::voice::VoiceSettingsState::in_memory_default();
        app.voice_settings_inputs
            .sync_from_state(&app.voice_settings, cx);
        app
    }

    pub fn new_with_pty_factory_and_voice_settings_store(
        cx: &mut Context<Self>,
        factory: runtime::PtyFactory,
        store: storage::SettingsStore,
    ) -> Self {
        let mut app = Self::new_with_pty_factory(cx, factory);
        app.voice_settings = crate::settings::voice::VoiceSettingsState::load_from_store(store);
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
            profile.api_key().is_some(),
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

    /// 以精确的逻辑时长推进 app 动效，供 headless e2e 验证帧边界使用。
    ///
    /// 真实窗口由 12ms 帧循环驱动；测试必须能直接验证 31ms 初始化帧和 80ms
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

    pub fn voice_e2e_snapshot(&self) -> VoiceE2eSnapshot {
        VoiceE2eSnapshot {
            phase: match self.voice.phase() {
                crate::app::voice::control::VoicePhase::Idle => "idle",
                crate::app::voice::control::VoicePhase::Recording { .. } => "recording",
                crate::app::voice::control::VoicePhase::Transcribing => "transcribing",
                crate::app::voice::control::VoicePhase::Planning { .. } => "planning",
                crate::app::voice::control::VoicePhase::Confirming { .. } => "confirming",
                crate::app::voice::control::VoicePhase::Executed { .. } => "executed",
                crate::app::voice::control::VoicePhase::TranscriptOnly { .. } => "transcript_only",
                crate::app::voice::control::VoicePhase::Error { .. } => "error",
            },
            dialogue_phase: match self.dialogue.phase() {
                dialogue::DialoguePhase::Idle => "idle",
                dialogue::DialoguePhase::Listening => "listening",
                dialogue::DialoguePhase::Thinking => "thinking",
                dialogue::DialoguePhase::Speaking => "speaking",
            },
            label: self.effective_voice_label(),
            microphone_recovery_available: self.can_open_microphone_privacy_settings(),
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

    /// 注入已被 TCC 拒绝的麦克风错误，供 L1 e2e 验证应用内恢复 UI。
    /// 系统设置本身属于 L3 外部流程，不能由 headless GPUI 修改。
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
        SettingsE2eSnapshot {
            settings_open: self.settings_panel.open,
            active_section: self.settings_panel.active_section.title(),
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
                .api_key()
                .is_some(),
            agent_base_url_input_surface_size: self
                .voice_settings_inputs
                .input_surface_size(crate::settings::voice::VoiceSettingsField::AgentBaseUrl, cx),
            agent_model_input_surface_size: self
                .voice_settings_inputs
                .input_surface_size(crate::settings::voice::VoiceSettingsField::AgentModel, cx),
            agent_base_url_rendered_text: self
                .voice_settings_inputs
                .rendered_text(crate::settings::voice::VoiceSettingsField::AgentBaseUrl, cx),
            agent_model_rendered_text: self
                .voice_settings_inputs
                .rendered_text(crate::settings::voice::VoiceSettingsField::AgentModel, cx),
            agent_api_key_display_state: classify_api_key_display(
                self.voice_settings_inputs
                    .rendered_text(crate::settings::voice::VoiceSettingsField::AgentApiKey, cx)
                    .as_deref(),
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
            tts_models: snapshot_tts_models(&self.voice_settings),
            tts_preview_active: self.tts_preview.is_active(),
            tts_configuration_generation: self.dialogue.tts_configuration_generation(),
        }
    }
}

fn opacity_to_milli(opacity: f32) -> u16 {
    (opacity.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn classify_api_key_display(text: Option<&str>) -> ApiKeyDisplayState {
    match text {
        None => ApiKeyDisplayState::NotRendered,
        Some("Not configured") => ApiKeyDisplayState::NotConfigured,
        Some("Configured ••••••••") => ApiKeyDisplayState::Configured,
        Some(value) if !value.is_empty() && value.chars().all(|character| character == '•') => {
            ApiKeyDisplayState::MaskedEditing
        }
        Some(_) => ApiKeyDisplayState::Unsafe,
    }
}

fn shortcut_command_name(command: ShortcutCommandId) -> &'static str {
    match command {
        ShortcutCommandId::NewWindow => "new_window",
        ShortcutCommandId::NewTerminal => "new_terminal",
        ShortcutCommandId::NewBrowser => "new_browser",
        ShortcutCommandId::CopyCardContent => "copy_card_content",
        ShortcutCommandId::OpenProject => "open_project",
        ShortcutCommandId::OpenPreviewFile => "open_preview_file",
        ShortcutCommandId::NewClaudeAgent => "new_claude_agent",
        ShortcutCommandId::NewCodexAgent => "new_codex_agent",
        ShortcutCommandId::NewCursorAgent => "new_cursor_agent",
        ShortcutCommandId::NewOpenCodeAgent => "new_opencode_agent",
        ShortcutCommandId::DeleteCard => "delete_card",
        ShortcutCommandId::ReflowLayout => "reflow_layout",
        ShortcutCommandId::ToggleMasterLayout => "toggle_master_layout",
        ShortcutCommandId::ExitFocus => "exit_focus",
        ShortcutCommandId::ToggleSettings => "toggle_settings",
    }
}
