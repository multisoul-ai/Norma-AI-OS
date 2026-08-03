use crate::app::{AppScreen, MultisoulApp};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::{VoiceInteractionMode, VoiceSettingsState};
use gpui::Context;
use ui::ThemeId;

const VISUAL_PREVIEW_ENV: &str = "NORMA_VISUAL_PREVIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisualPreview {
    AgentQueue,
    DispatchReceiptFlight,
    DispatchReceiptImpact,
    DispatchAgentHandoffFlight,
    Orchestration,
    GithubClone,
    FeatureGuide,
    VoiceSetup,
    VoiceReady,
    VoiceNetworkUnavailable,
    VoiceApiKey,
    VoiceCaption,
    VoiceWindowInset,
    AzureAsrReady,
    ZipVoiceReference,
    TtsBacklogCapacity,
    TtsBacklogRecovered,
}

impl VisualPreview {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "agent-queue" => Some(Self::AgentQueue),
            "dispatch-receipt-flight" => Some(Self::DispatchReceiptFlight),
            "dispatch-receipt-impact" => Some(Self::DispatchReceiptImpact),
            "dispatch-agent-handoff-flight" => Some(Self::DispatchAgentHandoffFlight),
            "orchestration" => Some(Self::Orchestration),
            "github-clone" => Some(Self::GithubClone),
            "feature-guide" => Some(Self::FeatureGuide),
            "voice-setup" => Some(Self::VoiceSetup),
            "voice-ready" => Some(Self::VoiceReady),
            "voice-network-unavailable" => Some(Self::VoiceNetworkUnavailable),
            "voice-api-key" => Some(Self::VoiceApiKey),
            "voice-caption" => Some(Self::VoiceCaption),
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
            Self::AgentQueue
            | Self::DispatchReceiptFlight
            | Self::DispatchReceiptImpact
            | Self::DispatchAgentHandoffFlight
            | Self::FeatureGuide => Some(ThemeId::PaperBlue),
            Self::Orchestration => Some(ThemeId::SapphireNight),
            Self::GithubClone => None,
            Self::VoiceSetup
            | Self::VoiceReady
            | Self::VoiceNetworkUnavailable
            | Self::VoiceApiKey
            | Self::VoiceCaption
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
    if preview == VisualPreview::GithubClone {
        app.screen = AppScreen::Launchpad;
        app.settings_panel.close();
    } else {
        app.screen = AppScreen::Canvas;
        app.settings_panel.open();
        app.settings_panel.select(SettingsSection::Voice);
    }
    match preview {
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
        VisualPreview::GithubClone => {}
        VisualPreview::FeatureGuide => {
            app.settings_panel.close();
            app.feature_guide.open_manually();
            app.sync_feature_guide_media(cx);
        }
        VisualPreview::VoiceSetup => {
            app.voice_settings = VoiceSettingsState::in_memory_default();
        }
        VisualPreview::VoiceReady => {
            app.voice_settings.assume_visual_preview_ready_for_e2e();
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
        }),
        source_bounds,
    );
    if agent_handoff {
        app.runtime.document.add_node(
            canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::Claude,
                alias: Some("老乔".to_string()),
                definition_id: Some(definition.id),
            }),
            target_bounds,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::VisualPreview;
    use ui::ThemeId;

    /// 视觉验收场景解析：只允许已登记的 Agent、Dispatch、Orchestration 与 Voice 截图状态
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   supported values = agent-queue(1) + dispatch-flight(1) + dispatch-impact(1)
    ///                      + agent-handoff-flight(1)
    ///                      + orchestration(1) + github-clone(1) + feature-guide(1)
    ///                      + setup(1) + ready(1) + network-unavailable(1)
    ///                      + api-key(1) + caption(1) + window-inset(1)
    ///                      + azure-asr-ready(1)
    ///                      + zipvoice-reference(1) + tts-backlog-capacity(1)
    ///                      + tts-backlog-recovered(1) = 17
    ///   unsupported values = empty(1) + unknown(1) = 2
    ///   total cases = 17 + 2 = 19
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析 `agent-queue` → AgentQueue
    ///   2. 解析 `dispatch-receipt-flight` → DispatchReceiptFlight
    ///   3. 解析 `dispatch-receipt-impact` → DispatchReceiptImpact
    ///   4. 解析 `dispatch-agent-handoff-flight` → DispatchAgentHandoffFlight
    ///   5. 解析 `orchestration` → Orchestration
    ///   6. 解析 `github-clone` → GithubClone
    ///   7. 解析 `feature-guide` → FeatureGuide
    ///   8. 解析 `voice-setup` → VoiceSetup
    ///   9. 解析 `voice-ready` → VoiceReady
    ///   10. 解析 `voice-network-unavailable` → VoiceNetworkUnavailable
    ///   11. 解析 `voice-api-key` → VoiceApiKey
    ///   12. 解析 `voice-caption` → VoiceCaption
    ///   13. 解析 `voice-window-inset` → VoiceWindowInset
    ///   14. 解析 `azure-asr-ready` → AzureAsrReady
    ///   15. 解析 `zipvoice-reference` → ZipVoiceReference
    ///   16. 解析 `tts-backlog-capacity` → TtsBacklogCapacity
    ///   17. 解析 `tts-backlog-recovered` → TtsBacklogRecovered
    ///   18. 解析空字符串 → None
    ///   19. 解析未知值 → None
    ///
    /// 预期结果：
    ///   - 正断言：十七个受支持值分别映射到唯一场景
    ///   - 负断言：空值和未知值不得意外开启视觉预览
    #[test]
    fn visual_preview_parser_accepts_only_explicit_scenarios() {
        assert_eq!(
            VisualPreview::parse("agent-queue"),
            Some(VisualPreview::AgentQueue),
            "agent-queue must select the canonical Agent queue sidecar preview"
        );
        assert_eq!(
            VisualPreview::parse("dispatch-receipt-flight"),
            Some(VisualPreview::DispatchReceiptFlight),
            "dispatch-receipt-flight must select the fixed mid-flight visual preview"
        );
        assert_eq!(
            VisualPreview::parse("dispatch-receipt-impact"),
            Some(VisualPreview::DispatchReceiptImpact),
            "dispatch-receipt-impact must select the fixed target explosion preview"
        );
        assert_eq!(
            VisualPreview::parse("dispatch-agent-handoff-flight"),
            Some(VisualPreview::DispatchAgentHandoffFlight),
            "dispatch-agent-handoff-flight must select the fixed Agent-to-Agent flight preview"
        );
        assert_eq!(
            VisualPreview::parse("orchestration"),
            Some(VisualPreview::Orchestration),
            "orchestration must select the canonical orchestration settings preview"
        );
        assert_eq!(
            VisualPreview::parse("github-clone"),
            Some(VisualPreview::GithubClone),
            "github-clone must select the canonical Clone Project modal preview"
        );
        assert_eq!(
            VisualPreview::parse("feature-guide"),
            Some(VisualPreview::FeatureGuide),
            "feature-guide must select the website-derived Feature Guide modal preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-setup"),
            Some(VisualPreview::VoiceSetup),
            "voice-setup must select the incomplete Voice Setup preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-ready"),
            Some(VisualPreview::VoiceReady),
            "voice-ready must select the completed Voice settings preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-network-unavailable"),
            Some(VisualPreview::VoiceNetworkUnavailable),
            "voice-network-unavailable must preserve the completed Voice settings preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-api-key"),
            Some(VisualPreview::VoiceApiKey),
            "voice-api-key must select the configured API Key preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-caption"),
            Some(VisualPreview::VoiceCaption),
            "voice-caption must select the long live transcript preview"
        );
        assert_eq!(
            VisualPreview::parse("voice-window-inset"),
            Some(VisualPreview::VoiceWindowInset),
            "voice-window-inset must select the local PCM-driven inset preview"
        );
        assert_eq!(
            VisualPreview::parse("azure-asr-ready"),
            Some(VisualPreview::AzureAsrReady),
            "azure-asr-ready must select the Ready Azure Settings preview"
        );
        assert_eq!(
            VisualPreview::parse("zipvoice-reference"),
            Some(VisualPreview::ZipVoiceReference),
            "zipvoice-reference must select the ready model with missing reference preview"
        );
        assert_eq!(
            VisualPreview::parse("tts-backlog-capacity"),
            Some(VisualPreview::TtsBacklogCapacity),
            "tts-backlog-capacity must select the visible spoken-reply capacity warning"
        );
        assert_eq!(
            VisualPreview::parse("tts-backlog-recovered"),
            Some(VisualPreview::TtsBacklogRecovered),
            "tts-backlog-recovered must select the stopped-shell recovery state"
        );
        assert_eq!(
            VisualPreview::parse(""),
            None,
            "an empty environment value must not alter normal application startup"
        );
        assert_eq!(
            VisualPreview::parse("voice-unknown"),
            None,
            "an unknown environment value must not expose an unintended preview state"
        );
    }

    /// Orchestration 视觉验收主题：canonical golden 的深色层级必须在隔离用户目录中仍可复现。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   visual preview = Orchestration(1)
    ///   canonical theme = SapphireNight(1)
    ///   用户持久化主题输入 = 0（固定验收不得依赖本机历史设置）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 读取 Orchestration 视觉预览的 canonical theme。
    ///   2. 将结果与 golden 使用的 SapphireNight 精确比较。
    ///
    /// 预期结果：
    ///   - 正断言：Orchestration 固定使用 SapphireNight。
    ///   - 负断言：不得回退到默认浅色 PaperBlue。
    #[test]
    fn orchestration_visual_preview_uses_canonical_dark_theme() {
        let theme = VisualPreview::Orchestration.canonical_theme();

        assert_eq!(
            theme,
            Some(ThemeId::SapphireNight),
            "Orchestration 视觉预览必须固定使用 golden 对应的 SapphireNight"
        );
        assert_ne!(
            theme,
            Some(ThemeId::PaperBlue),
            "隔离 HOME 或新用户默认值不得把 canonical preview 变成浅色"
        );
    }
}
