use crate::app::{AppScreen, MultisoulApp};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::{DesktopVoiceMode, VoiceInteractionMode, VoiceSettingsState};
use gpui::Context;
use ui::ThemeId;

mod working_snake;

const VISUAL_PREVIEW_ENV: &str = "NORMA_VISUAL_PREVIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisualPreview {
    WorkingSnake,
    AgentQueue,
    DispatchReceiptFlight,
    DispatchReceiptImpact,
    DispatchAgentHandoffFlight,
    Orchestration,
    Launchpad,
    GithubClone,
    FeatureGuide,
    VoiceSetup,
    VoiceReady,
    VoiceCatalog,
    VoiceNetworkUnavailable,
    VoiceApiKey,
    VoiceCaption,
    VoiceWaveformListening,
    VoiceWaveformThinking,
    VoiceWaveformReplying,
    VoiceWindowInset,
    AzureAsrReady,
    ZipVoiceReference,
    TtsBacklogCapacity,
    TtsBacklogRecovered,
}

impl VisualPreview {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "working-snake" => Some(Self::WorkingSnake),
            "agent-queue" => Some(Self::AgentQueue),
            "dispatch-receipt-flight" => Some(Self::DispatchReceiptFlight),
            "dispatch-receipt-impact" => Some(Self::DispatchReceiptImpact),
            "dispatch-agent-handoff-flight" => Some(Self::DispatchAgentHandoffFlight),
            "orchestration" => Some(Self::Orchestration),
            "launchpad" => Some(Self::Launchpad),
            "github-clone" => Some(Self::GithubClone),
            "feature-guide" => Some(Self::FeatureGuide),
            "voice-setup" => Some(Self::VoiceSetup),
            "voice-ready" => Some(Self::VoiceReady),
            "voice-catalog" => Some(Self::VoiceCatalog),
            "voice-network-unavailable" => Some(Self::VoiceNetworkUnavailable),
            "voice-api-key" => Some(Self::VoiceApiKey),
            "voice-caption" => Some(Self::VoiceCaption),
            "voice-waveform-listening" => Some(Self::VoiceWaveformListening),
            "voice-waveform-thinking" => Some(Self::VoiceWaveformThinking),
            "voice-waveform-replying" => Some(Self::VoiceWaveformReplying),
            "voice-window-inset" => Some(Self::VoiceWindowInset),
            "azure-asr-ready" => Some(Self::AzureAsrReady),
            "zipvoice-reference" => Some(Self::ZipVoiceReference),
            "tts-backlog-capacity" => Some(Self::TtsBacklogCapacity),
            "tts-backlog-recovered" => Some(Self::TtsBacklogRecovered),
            _ => None,
        }
    }

    fn canonical_theme(self) -> Option<ThemeId> {
        match self {
            Self::WorkingSnake
            | Self::AgentQueue
            | Self::DispatchReceiptFlight
            | Self::DispatchReceiptImpact
            | Self::DispatchAgentHandoffFlight
            | Self::FeatureGuide => Some(ThemeId::PaperBlue),
            Self::Orchestration => Some(ThemeId::SapphireNight),
            Self::Launchpad | Self::GithubClone => None,
            Self::VoiceSetup
            | Self::VoiceReady
            | Self::VoiceCatalog
            | Self::VoiceNetworkUnavailable
            | Self::VoiceApiKey
            | Self::VoiceCaption
            | Self::VoiceWaveformListening
            | Self::VoiceWaveformThinking
            | Self::VoiceWaveformReplying
            | Self::VoiceWindowInset
            | Self::AzureAsrReady
            | Self::ZipVoiceReference
            | Self::TtsBacklogCapacity
            | Self::TtsBacklogRecovered => None,
        }
    }
}

pub(super) fn configure_from_env(app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>) -> bool {
    let Some(preview) = std::env::var(VISUAL_PREVIEW_ENV)
        .ok()
        .as_deref()
        .and_then(VisualPreview::parse)
    else {
        return false;
    };

    if let Some(theme_id) = preview.canonical_theme() {
        app.set_theme(theme_id, cx);
    }
    if matches!(
        preview,
        VisualPreview::Launchpad | VisualPreview::GithubClone
    ) {
        app.screen = AppScreen::Launchpad;
        app.settings_panel.close();
    } else {
        app.screen = AppScreen::Canvas;
        app.settings_panel.open();
        app.settings_panel.select(SettingsSection::Voice);
    }
    match preview {
        VisualPreview::WorkingSnake => {
            app.settings_panel.close();
            working_snake::configure(app);
        }
        VisualPreview::AgentQueue => {
            app.settings_panel.close();
            configure_agent_queue(app);
        }
        VisualPreview::DispatchReceiptFlight | VisualPreview::DispatchReceiptImpact => {
            app.settings_panel.close();
            configure_dispatch_receipt(app, false);
        }
        VisualPreview::DispatchAgentHandoffFlight => {
            app.settings_panel.close();
            configure_dispatch_receipt(app, true);
        }
        VisualPreview::Orchestration => {
            app.settings_panel.select(SettingsSection::Orchestration);
            app.orchestration_settings.use_canonical_preview();
        }
        VisualPreview::Launchpad => {}
        VisualPreview::GithubClone => {}
        VisualPreview::FeatureGuide => {
            app.settings_panel.close();
            app.orchestration_settings.use_canonical_preview();
            app.feature_guide.open_manually();
            app.sync_feature_guide_media(cx);
        }
        VisualPreview::VoiceSetup => {
            app.voice_settings = VoiceSettingsState::in_memory_default();
        }
        VisualPreview::VoiceReady => {
            app.voice_settings.assume_visual_preview_ready_for_e2e();
        }
        VisualPreview::VoiceCatalog => {
            app.voice_settings = VoiceSettingsState::in_memory_default();
            app.voice_settings.assume_visual_preview_ready_for_e2e();
            app.assume_minimax_connected_for_e2e(cx);
            app.set_fake_tts_transport_for_e2e(true, false, cx);
            app.voice_settings.open_tts_picker();
            app.voice_settings
                .select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
        }
        VisualPreview::VoiceNetworkUnavailable => {
            app.voice_settings
                .assume_minimax_temporary_network_failure_for_e2e();
        }
        VisualPreview::VoiceApiKey => {
            app.voice_settings.assume_visual_preview_api_key_for_e2e();
        }
        VisualPreview::VoiceCaption => {
            app.settings_panel.close();
            app.voice_settings.assume_visual_preview_ready_for_e2e();
            app.voice.set_transcript_only(
                "请帮我整理今天的项目进展，并根据优先级列出下一步行动。我还想确认语音记录不会因为内容太长而丢失。接下来请检查所有未完成事项，标注负责人和截止日期，并把风险最高的三项单独列出来。",
                "voice-caption visual preview",
            );
        }
        VisualPreview::VoiceWaveformListening
        | VisualPreview::VoiceWaveformThinking
        | VisualPreview::VoiceWaveformReplying => {
            app.settings_panel.close();
            app.voice_settings.assume_visual_preview_ready_for_e2e();
            app.voice_settings
                .select_desktop_mode(DesktopVoiceMode::Doubao);
            app.doubao.status = match preview {
                VisualPreview::VoiceWaveformListening => {
                    crate::app::doubao::DoubaoStatus::Connected
                }
                VisualPreview::VoiceWaveformThinking => crate::app::doubao::DoubaoStatus::Thinking,
                VisualPreview::VoiceWaveformReplying => crate::app::doubao::DoubaoStatus::Speaking,
                _ => unreachable!(),
            };
        }
        VisualPreview::VoiceWindowInset => {
            app.settings_panel.close();
            app.voice_settings.assume_visual_preview_ready_for_e2e();
            app.begin_assistant_playback_for_e2e(cx);
            // 30s × 16_000Hz = 480_000 mono frames；每帧 PCM16 = 2 bytes，
            // 共 960_000 bytes。每 200ms = 3_200 frames 切换一次能量档位，
            // 让人工验收能观察响应变化，同时不触发真实 TTS/音频输出。
            let amplitudes = [2_000_i16, 8_000_i16, 14_000_i16, 5_000_i16];
            let bytes = (0..480_000)
                .flat_map(|frame| amplitudes[(frame / 3_200) % amplitudes.len()].to_le_bytes())
                .collect();
            let chunk = audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, bytes)
                .expect("Voice inset visual preview PCM must be valid 16kHz mono PCM16");
            app.queue_assistant_pcm_for_e2e(chunk, cx);
        }
        VisualPreview::AzureAsrReady => {
            app.voice_settings
                .assume_visual_preview_azure_asr_ready_for_e2e();
        }
        VisualPreview::ZipVoiceReference => {
            app.voice_settings
                .assume_visual_preview_zipvoice_reference_for_e2e();
        }
        VisualPreview::TtsBacklogCapacity => {
            app.settings_panel.close();
            app.voice_settings.assume_visual_preview_ready_for_e2e();
            app.voice_settings
                .select_interaction_mode(VoiceInteractionMode::ContinuousListening);
            app.dialogue.recover_reply_failure(
                crate::app::dialogue::spoken_reply_capacity_message(
                    dialogue::MAX_ACTIVE_SPEECH_CHARS,
                ),
            );
        }
        VisualPreview::TtsBacklogRecovered => {
            app.settings_panel.close();
            app.dialogue.recover_reply_failure(
                crate::app::dialogue::spoken_reply_capacity_message(
                    dialogue::MAX_ACTIVE_SPEECH_CHARS,
                ),
            );
            app.dialogue.stop_shell();
        }
    }
    app.voice_settings_inputs
        .sync_from_state(&app.voice_settings, cx);
    cx.notify();
    true
}

fn configure_agent_queue(app: &mut MultisoulApp) {
    // initial_window_dimensions(agent-queue) = 1280 × 800；预先记录该尺寸，
    // 避免首帧的 tiled reflow 把 620px 原型卡片扩张为全画布卡片。
    app.last_reflow_window_size = Some((1280.0, 800.0));
    let definition = app.agent_definitions.use_queue_visual_preview();
    let node_id = app.runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some("Adam".to_string()),
            definition_id: Some(definition.id),
            mobile_control: false,
        }),
        canvas::CanvasRect::new(80.0, 120.0, 620.0, 420.0),
    );
    let roster = vec![agent::AgentRosterEntry {
        definition_id: definition.id,
        name: definition.name,
        speech_alias: definition.speech_alias,
        profile: norma::protocol::AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(node_id),
        tracking: agent::AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }];
    let mut coordinator = agent::AgentTaskCoordinator::new(roster)
        .expect("queue visual preview roster should be valid");
    coordinator
        .dispatch(
            vec![agent::AgentAssignment::new("Adam", "重构用户认证流程")],
            false,
        )
        .expect("queue visual preview current task should dispatch");
    coordinator
        .dispatch_from(
            vec![agent::AgentAssignment::new("Adam", "补充失败路径测试")],
            false,
            agent::AgentDispatchSource::manual(),
        )
        .expect("queue visual preview first waiting task should dispatch");
    coordinator
        .dispatch(
            vec![agent::AgentAssignment::new("Adam", "更新 API 文档")],
            false,
        )
        .expect("queue visual preview second waiting task should dispatch");
    coordinator
        .dispatch_from(
            vec![agent::AgentAssignment::new("Adam", "运行回归检查")],
            false,
            agent::AgentDispatchSource::manual(),
        )
        .expect("queue visual preview third waiting task should dispatch");
    app.agent_task_coordinator = Some(coordinator);
    app.agent_queue_ui.toggle(node_id);
}

fn configure_dispatch_receipt(app: &mut MultisoulApp, agent_handoff: bool) {
    app.last_reflow_window_size = Some((1280.0, 800.0));
    let definition = app.agent_definitions.use_queue_visual_preview();
    let (source_bounds, target_bounds) = if agent_handoff {
        (
            canvas::CanvasRect::new(70.0, 170.0, 500.0, 360.0),
            canvas::CanvasRect::new(710.0, 190.0, 500.0, 360.0),
        )
    } else {
        (
            canvas::CanvasRect::new(120.0, 140.0, 620.0, 400.0),
            canvas::CanvasRect::new(120.0, 140.0, 620.0, 400.0),
        )
    };
    app.runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some(if agent_handoff { "浩宇" } else { "Adam" }.to_string()),
            definition_id: Some(definition.id),
            mobile_control: false,
        }),
        source_bounds,
    );
    if agent_handoff {
        app.runtime.document.add_node(
            canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::Claude,
                alias: Some("老乔".to_string()),
                definition_id: Some(definition.id),
                mobile_control: false,
            }),
            target_bounds,
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/visual_preview_tests.rs"]
mod tests;
