//! Stable, primitive e2e projections that do not expose private settings models.

use super::{AppScreen, MultisoulApp};
use crate::shortcuts::ShortcutCommandId;
use gpui::{App, Context};
#[cfg(feature = "e2e-test-support")]
use std::path::PathBuf;
use voice_sherpa::SpeechError;

#[cfg(feature = "e2e-test-support")]
mod agent_announcements;
mod agents;
#[cfg(feature = "e2e-test-support")]
mod browser;
#[cfg(any(test, feature = "e2e-test-support"))]
mod database;
#[cfg(feature = "e2e-test-support")]
pub mod dialogue_tts;
#[cfg(feature = "e2e-test-support")]
mod ego;
mod exports;
#[cfg(feature = "e2e-test-support")]
mod orchestration;
mod recent_projects;
#[cfg(feature = "e2e-test-support")]
mod repaint;
mod shortcuts;
mod tts;
mod visuals;
mod voice;
#[cfg(feature = "e2e-test-support")]
mod voice_setup;
pub use exports::*;
use shortcuts::shortcut_command_name;
use tts::{snapshot_tts_models, snapshot_tts_provider, TtsModelE2eSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanvasLayoutE2eSnapshot {
    pub menu_open: bool,
    pub fill: &'static str,
    pub track_count: usize,
    pub sizing: &'static str,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeatureGuideE2eSnapshot {
    pub visible: bool,
    pub status: &'static str,
    pub presentation: &'static str,
    pub topic: &'static str,
    pub media_playing: bool,
    pub poster_available: bool,
    pub primary_focused: bool,
    pub orchestration_details_expanded: bool,
    pub topic_position: usize,
    pub topic_count: usize,
}

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
    pub idle_animation_throttling_enabled: bool,
    pub active_theme_id: &'static str,
    pub theme_is_dark: bool,
    pub theme_canvas_from: u32,
    pub theme_accent: u32,
    pub recording_command: Option<&'static str>,
    pub new_terminal_shortcut: Option<String>,
    pub new_terminal_modified: bool,
    pub voice_interaction_shortcut: Option<String>,
    pub voice_interaction_modified: bool,
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
    pub wallpaper_preview_cache_entries: usize,
    pub wallpaper_preview_cache_bytes: usize,
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

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloneProjectE2eSnapshot {
    pub open: bool,
    pub phase: &'static str,
    pub transport: &'static str,
    pub remote: String,
    pub folder: String,
    pub parent: Option<PathBuf>,
    pub target_preview: Option<PathBuf>,
    pub validation_error: Option<String>,
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn use_successful_clone_backend_for_e2e(&mut self) {
        self.clone_project.use_successful_fake_backend();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn set_clone_parent_for_e2e(&mut self, parent: PathBuf, cx: &mut Context<Self>) {
        self.clone_project.set_parent_for_e2e(parent);
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn clone_project_e2e_snapshot(&self) -> CloneProjectE2eSnapshot {
        let snapshot: crate::workspace::clone_project::CloneProjectInternalE2eSnapshot =
            self.clone_project.e2e_snapshot();
        CloneProjectE2eSnapshot {
            open: snapshot.open,
            phase: snapshot.phase,
            transport: snapshot.transport,
            remote: snapshot.remote,
            folder: snapshot.folder,
            parent: snapshot.parent,
            target_preview: snapshot.target_preview,
            validation_error: snapshot.validation_error,
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn feature_guide_e2e_snapshot(&self) -> FeatureGuideE2eSnapshot {
        use super::feature_guide::FeatureGuidePresentation;
        FeatureGuideE2eSnapshot {
            visible: self.feature_guide.is_visible(),
            status: self.feature_guide.serialized_status(),
            presentation: match self.feature_guide.presentation {
                FeatureGuidePresentation::Idle => "idle",
                FeatureGuidePresentation::Pending => "pending",
                FeatureGuidePresentation::Visible => "visible",
                FeatureGuidePresentation::Suspended => "suspended",
                FeatureGuidePresentation::ClosedForSession => "closed_for_session",
            },
            topic: self.feature_guide.serialized_topic(),
            media_playing: self.feature_guide_media.is_playing(),
            poster_available: self.feature_guide_media.poster_available(),
            primary_focused: self.feature_guide_primary_focused_for_e2e,
            orchestration_details_expanded: self.feature_guide.orchestration_details_expanded(),
            topic_position: self.feature_guide.topic_position(),
            topic_count: super::feature_guide::FeatureGuideTopic::ALL.len(),
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn enable_fresh_feature_guide_for_e2e(&mut self) {
        self.feature_guide = super::feature_guide::FeatureGuideState::load(None, None, false);
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn set_feature_guide_reduce_motion_for_e2e(
        &mut self,
        reduce_motion: bool,
        cx: &mut Context<Self>,
    ) {
        self.motion.reduce_motion = reduce_motion;
        self.sync_feature_guide_media(cx);
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn present_feature_guide_after_canvas_ready_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.on_workspace_opened();
        self.present_feature_guide_if_ready(cx);
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn set_voice_model_root_for_e2e(&mut self, model_root: PathBuf, cx: &mut Context<Self>) {
        self.voice_settings.reset_model_root_for_e2e(model_root);
        self.refresh_tts_model_states(cx);
        cx.notify();
    }

    /// 聚焦卡片当前是否应推进 terminal cursor 动画；Agent 与 Terminal 都按 PTY 存活判断。
    pub fn focused_terminal_cursor_animation_active_e2e(&self) -> bool {
        self.focused_terminal_cursor_animation_active()
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub fn disable_agent_hook_for_test(&mut self) {
        self.agent_hook_server.disable_for_test();
    }

    pub fn screen(&self) -> AppScreen {
        self.screen
    }

    pub fn canvas_layout_e2e_snapshot(&self) -> CanvasLayoutE2eSnapshot {
        CanvasLayoutE2eSnapshot {
            menu_open: self.interaction.layout_menu_open,
            fill: match self.layout_strategy.fill {
                canvas::GridFill::RowFirst => "row_first",
                canvas::GridFill::ColumnFirst => "column_first",
            },
            track_count: self.layout_strategy.track_count,
            sizing: match self.layout_strategy.sizing {
                canvas::GridCardSizing::Fixed => "fixed",
                canvas::GridCardSizing::FitViewport => "fit_viewport",
            },
        }
    }

    /// Launchpad 的固定背景资源；Canvas 的 Wallpaper 设置不会改变此值。
    pub fn launchpad_video_asset_e2e(&self) -> &'static str {
        crate::workspace::launchpad::launchpad_default_video_asset()
    }

    pub fn enter_canvas_for_test(&mut self, root: &std::path::Path, cx: &mut Context<Self>) {
        let project_store =
            storage::ProjectStore::initialize_for_database_paths(&self.database_paths, root)
                .expect("e2e workspace root should initialize test project storage");
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
        let voice_interaction = self
            .shortcuts
            .effective
            .iter()
            .find(|entry| entry.command == ShortcutCommandId::VoiceInteraction);
        let api_key_text = self
            .voice_settings_inputs
            .rendered_text(crate::settings::voice::VoiceSettingsField::ApiKey, cx);
        let api_key_profile = self.voice_settings.values().active_provider_profile();
        let runtime_tts = self.dialogue.selected_tts_model.clone();
        SettingsE2eSnapshot {
            settings_open: self.settings_panel.open,
            active_section: self.settings_panel.active_section.title(),
            idle_animation_throttling_enabled: self
                .performance_settings
                .idle_animation_throttling_enabled(),
            active_theme_id: self.theme_id.as_str(),
            theme_is_dark: self.theme_id.is_dark(),
            theme_canvas_from: self.theme.canvas_from,
            theme_accent: self.theme.accent,
            recording_command: self.shortcuts.recording.map(shortcut_command_name),
            new_terminal_shortcut: new_terminal.and_then(|entry| entry.current_keystroke.clone()),
            new_terminal_modified: new_terminal.map(|entry| entry.modified).unwrap_or(false),
            voice_interaction_shortcut: voice_interaction
                .and_then(|entry| entry.current_keystroke.clone()),
            voice_interaction_modified: voice_interaction
                .map(|entry| entry.modified)
                .unwrap_or(false),
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
            wallpaper_preview_cache_entries: self.wallpaper_previews.len(),
            wallpaper_preview_cache_bytes: self.wallpaper_previews.total_bytes(),
        }
    }
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
