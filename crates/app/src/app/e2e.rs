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
use std::time::Duration;

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
}

impl MultisoulApp {
    pub fn new_with_pty_factory(cx: &mut Context<Self>, factory: runtime::PtyFactory) -> Self {
        let agent_hook_server = AgentHookServer::start();
        let mut runtime = Runtime::new();
        runtime.set_pty_factory(factory);
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        let mut app = Self::build(cx, runtime, agent_hook_server);
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
                        crate::motion::InitializationAnimationKind::Agent => "agent",
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
            },
            None => VoiceWindowInsetE2eSnapshot {
                active: false,
                opacity_milli: 0,
                inset_px_milli: 0,
            },
        }
    }

    pub fn enter_canvas_for_test(&mut self, root: &std::path::Path, cx: &mut Context<Self>) {
        self.runtime.set_workspace_root(root.to_path_buf());
        self.screen = AppScreen::Canvas;
        self.last_reflow_window_size = None;
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
        ShortcutCommandId::NewOpenCodeAgent => "new_opencode_agent",
        ShortcutCommandId::DeleteCard => "delete_card",
        ShortcutCommandId::ReflowLayout => "reflow_layout",
        ShortcutCommandId::ToggleMasterLayout => "toggle_master_layout",
        ShortcutCommandId::ExitFocus => "exit_focus",
        ShortcutCommandId::ToggleSettings => "toggle_settings",
    }
}
