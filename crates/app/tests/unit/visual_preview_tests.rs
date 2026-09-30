use super::VisualPreview;
use ui::ThemeId;

/// 视觉验收场景解析：只允许已登记的 Agent、Dispatch、Orchestration 与 Voice 截图状态
///
/// 数据构造（含关键数值的推导过程）：
///   supported values = working-snake(1) + agent-queue(1)
///                      + dispatch-flight(1) + dispatch-impact(1)
///                      + agent-handoff-flight(1)
///                      + orchestration(1) + launchpad(1) + github-clone(1) + feature-guide(1)
///                      + setup(1) + ready(1) + catalog(1) + network-unavailable(1)
///                      + api-key(1) + caption(1) + waveform(listening/thinking/replying)(3)
///                      + window-inset(1)
///                      + azure-asr-ready(1)
///                      + zipvoice-reference(1) + tts-backlog-capacity(1)
///                      + tts-backlog-recovered(1) = 23
///   unsupported values = empty(1) + unknown(1) = 2
///   total cases = 23 + 2 = 25
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `working-snake` → WorkingSnake
///   2. 解析 `agent-queue` → AgentQueue
///   3. 继续解析其余二十一个已登记场景 → 各自唯一枚举值
///   4. 解析空字符串 → None
///   5. 解析未知值 → None
///
/// 预期结果：
///   - 正断言：二十三个受支持值分别映射到唯一场景
///   - 负断言：Launchpad 不得复用 GithubClone；空值和未知值不得开启视觉预览
#[test]
fn visual_preview_parser_accepts_only_explicit_scenarios() {
    assert_eq!(
        VisualPreview::parse("working-snake"),
        Some(VisualPreview::WorkingSnake),
        "working-snake must select the deterministic active Agent visual preview"
    );
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
        VisualPreview::parse("launchpad"),
        Some(VisualPreview::Launchpad),
        "launchpad must select the isolated home screen without a blocking dialog"
    );
    assert_ne!(
        VisualPreview::parse("launchpad"),
        Some(VisualPreview::GithubClone),
        "launchpad must not reuse the preview that automatically opens the clone dialog"
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
        VisualPreview::parse("voice-catalog"),
        Some(VisualPreview::VoiceCatalog),
        "voice-catalog must provide an isolated native voice list for Computer Use acceptance"
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
        VisualPreview::parse("voice-waveform-listening"),
        Some(VisualPreview::VoiceWaveformListening),
        "listening preview must show the active voice waveform"
    );
    assert_eq!(
        VisualPreview::parse("voice-waveform-thinking"),
        Some(VisualPreview::VoiceWaveformThinking),
        "thinking preview must show the waiting voice waveform"
    );
    assert_eq!(
        VisualPreview::parse("voice-waveform-replying"),
        Some(VisualPreview::VoiceWaveformReplying),
        "replying preview must show the assistant voice waveform"
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
