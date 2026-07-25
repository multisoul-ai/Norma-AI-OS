use crate::app::{AppScreen, MultisoulApp};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::VoiceSettingsState;
use gpui::Context;
use ui::ThemeId;

const VISUAL_PREVIEW_ENV: &str = "NORMA_VISUAL_PREVIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisualPreview {
    Orchestration,
    VoiceSetup,
    VoiceReady,
    VoiceApiKey,
    AzureAsrReady,
    ZipVoiceReference,
    TtsBacklogCapacity,
    TtsBacklogRecovered,
}

impl VisualPreview {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "orchestration" => Some(Self::Orchestration),
            "voice-setup" => Some(Self::VoiceSetup),
            "voice-ready" => Some(Self::VoiceReady),
            "voice-api-key" => Some(Self::VoiceApiKey),
            "azure-asr-ready" => Some(Self::AzureAsrReady),
            "zipvoice-reference" => Some(Self::ZipVoiceReference),
            "tts-backlog-capacity" => Some(Self::TtsBacklogCapacity),
            "tts-backlog-recovered" => Some(Self::TtsBacklogRecovered),
            _ => None,
        }
    }

    fn canonical_theme(self) -> Option<ThemeId> {
        match self {
            Self::Orchestration => Some(ThemeId::SapphireNight),
            Self::VoiceSetup
            | Self::VoiceReady
            | Self::VoiceApiKey
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
    app.screen = AppScreen::Canvas;
    app.settings_panel.open();
    app.settings_panel.select(SettingsSection::Voice);
    match preview {
        VisualPreview::Orchestration => {
            app.settings_panel.select(SettingsSection::Orchestration);
            app.orchestration_settings.use_canonical_preview();
        }
        VisualPreview::VoiceSetup => {
            app.voice_settings = VoiceSettingsState::in_memory_default();
        }
        VisualPreview::VoiceReady => {
            app.voice_settings.assume_visual_preview_ready_for_e2e();
        }
        VisualPreview::VoiceApiKey => {
            app.voice_settings.assume_visual_preview_api_key_for_e2e();
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

#[cfg(test)]
mod tests {
    use super::VisualPreview;
    use ui::ThemeId;

    /// 视觉验收场景解析：只允许一个 Orchestration 与七个明确的 Voice 截图状态
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   supported values = orchestration(1) + setup(1) + ready(1) + api-key(1)
    ///                      + azure-asr-ready(1)
    ///                      + zipvoice-reference(1) + tts-backlog-capacity(1)
    ///                      + tts-backlog-recovered(1) = 8
    ///   unsupported values = empty(1) + unknown(1) = 2
    ///   total cases = 8 + 2 = 10
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析 `orchestration` → Orchestration
    ///   2. 解析 `voice-setup` → VoiceSetup
    ///   3. 解析 `voice-ready` → VoiceReady
    ///   4. 解析 `voice-api-key` → VoiceApiKey
    ///   5. 解析 `azure-asr-ready` → AzureAsrReady
    ///   6. 解析 `zipvoice-reference` → ZipVoiceReference
    ///   7. 解析 `tts-backlog-capacity` → TtsBacklogCapacity
    ///   8. 解析 `tts-backlog-recovered` → TtsBacklogRecovered
    ///   9. 解析空字符串与未知值 → None
    ///
    /// 预期结果：
    ///   - 正断言：八个受支持值分别映射到唯一场景
    ///   - 负断言：空值和未知值不得意外开启视觉预览
    #[test]
    fn visual_preview_parser_accepts_only_explicit_voice_scenarios() {
        assert_eq!(
            VisualPreview::parse("orchestration"),
            Some(VisualPreview::Orchestration),
            "orchestration must select the canonical orchestration settings preview"
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
            VisualPreview::parse("voice-api-key"),
            Some(VisualPreview::VoiceApiKey),
            "voice-api-key must select the configured API Key preview"
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
