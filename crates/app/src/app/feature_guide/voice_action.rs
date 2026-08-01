use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceEntryState, VoiceRequirement};
use gpui::{prelude::*, px, rgba, AnyElement, Context};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::component::Disableable;
use ui::Theme;

use super::style::with_alpha;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeatureGuideVoiceAction {
    TryNow,
    AllowMicrophone,
    Configure,
    Preparing,
    Repair,
}

impl FeatureGuideVoiceAction {
    pub(crate) fn from_readiness(readiness: VoiceEntryState) -> Self {
        match readiness {
            VoiceEntryState::Ready => Self::TryNow,
            VoiceEntryState::SettingUp { .. } => Self::Preparing,
            VoiceEntryState::SetUp {
                first_incomplete: VoiceRequirement::Microphone,
            } => Self::AllowMicrophone,
            VoiceEntryState::SetUp { .. } => Self::Configure,
            VoiceEntryState::Unavailable { .. } => Self::Repair,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TryNow => "现在试试",
            Self::AllowMicrophone => "允许麦克风",
            Self::Configure => "配置语音",
            Self::Preparing => "正在准备语音",
            Self::Repair => "修复语音",
        }
    }

    fn enabled(self) -> bool {
        self != Self::Preparing
    }
}

pub(super) fn render_voice_action(
    app: &MultisoulApp,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let action = FeatureGuideVoiceAction::from_readiness(app.voice_settings.readiness());
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.accent_violet, 0x12)).into())
        .foreground(rgba(theme.accent_violet).into())
        .border(rgba(with_alpha(theme.accent_violet, 0x58)).into())
        .hover(rgba(with_alpha(theme.accent_violet, 0x22)).into())
        .active(rgba(with_alpha(theme.accent_violet, 0x32)).into());
    Button::new("feature-guide-voice-action")
        .debug_selector(|| "feature-guide-voice-action".to_string())
        .label(action.label())
        .tooltip(action.label())
        .tab_index(6)
        .disabled(!action.enabled())
        .custom(variant)
        .h(px(36.0))
        .px(px(14.0))
        .rounded(px(10.0))
        .on_click(cx.listener(move |app, _, window, cx| {
            cx.stop_propagation();
            match action {
                FeatureGuideVoiceAction::TryNow => {
                    app.close_feature_guide_for_session(cx);
                    app.activate_ready_voice(true, window, cx);
                }
                FeatureGuideVoiceAction::AllowMicrophone
                | FeatureGuideVoiceAction::Configure
                | FeatureGuideVoiceAction::Repair => app.open_voice_settings(window, cx),
                FeatureGuideVoiceAction::Preparing => {}
            }
        }))
        .into_any_element()
}
