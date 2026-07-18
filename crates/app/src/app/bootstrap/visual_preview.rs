use crate::app::{AppScreen, MultisoulApp};
use crate::settings::panel::SettingsSection;
use crate::settings::voice::VoiceSettingsState;
use gpui::Context;

const VISUAL_PREVIEW_ENV: &str = "NORMA_VISUAL_PREVIEW";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisualPreview {
    VoiceSetup,
    VoiceReady,
    VoiceApiKey,
    ZipVoiceReference,
}

impl VisualPreview {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "voice-setup" => Some(Self::VoiceSetup),
            "voice-ready" => Some(Self::VoiceReady),
            "voice-api-key" => Some(Self::VoiceApiKey),
            "zipvoice-reference" => Some(Self::ZipVoiceReference),
            _ => None,
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

    app.screen = AppScreen::Canvas;
    app.settings_panel.open();
    app.settings_panel.select(SettingsSection::Voice);
    match preview {
        VisualPreview::VoiceSetup => {
            app.voice_settings = VoiceSettingsState::in_memory_default();
        }
        VisualPreview::VoiceReady => {
            app.voice_settings.assume_visual_preview_ready_for_e2e();
        }
        VisualPreview::VoiceApiKey => {
            app.voice_settings.assume_visual_preview_api_key_for_e2e();
        }
        VisualPreview::ZipVoiceReference => {
            app.voice_settings
                .assume_visual_preview_zipvoice_reference_for_e2e();
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

    /// 视觉验收场景解析：只允许四个明确的 Voice 截图状态
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   supported values = setup(1) + ready(1) + api-key(1) + zipvoice-reference(1) = 4
    ///   unsupported values = empty(1) + unknown(1) = 2
    ///   total cases = 4 + 2 = 6
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 解析 `voice-setup` → VoiceSetup
    ///   2. 解析 `voice-ready` → VoiceReady
    ///   3. 解析 `voice-api-key` → VoiceApiKey
    ///   4. 解析 `zipvoice-reference` → ZipVoiceReference
    ///   5. 解析空字符串与未知值 → None
    ///
    /// 预期结果：
    ///   - 正断言：四个受支持值分别映射到唯一场景
    ///   - 负断言：空值和未知值不得意外开启视觉预览
    #[test]
    fn visual_preview_parser_accepts_only_explicit_voice_scenarios() {
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
            VisualPreview::parse("zipvoice-reference"),
            Some(VisualPreview::ZipVoiceReference),
            "zipvoice-reference must select the ready model with missing reference preview"
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
}
