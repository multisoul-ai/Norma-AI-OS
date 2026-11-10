//! 语音光晕：风格、相位与偏好。纯逻辑，风格只产出图元，由 `app::voice_window_inset` 负责绘制。

pub(crate) mod scene;
pub(crate) mod styles;

/// 键：风格 / 强度 / 聆听与思考是否显示。全局偏好，所有项目共用。
pub(crate) const KEY_STYLE: &str = "effects.voice_glow.style";
pub(crate) const KEY_GAIN: &str = "effects.voice_glow.gain";
pub(crate) const KEY_PHASES: &str = "effects.voice_glow.phases";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceGlowStyle {
    Off,
    #[default]
    Classic,
    Halo,
    Ribbon,
    Stardust,
}

impl VoiceGlowStyle {
    pub(crate) const ALL: [Self; 5] = [
        Self::Off,
        Self::Classic,
        Self::Halo,
        Self::Ribbon,
        Self::Stardust,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Classic => "classic",
            Self::Halo => "halo",
            Self::Ribbon => "ribbon",
            Self::Stardust => "stardust",
        }
    }

    /// 未配置或未知值回落到经典，保证升级后效果不变。
    pub(crate) fn from_id(id: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|style| Some(style.id()) == id)
            .unwrap_or_default()
    }

    /// 旧开关 `voice.window_inset_enabled`：关闭过的用户保持关闭，其余为经典。
    pub(crate) fn migrate_legacy(legacy_enabled: Option<bool>) -> Self {
        if legacy_enabled == Some(false) {
            Self::Off
        } else {
            Self::Classic
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Gain {
    Soft,
    #[default]
    Standard,
    Vivid,
}

impl Gain {
    pub(crate) const ALL: [Self; 3] = [Self::Soft, Self::Standard, Self::Vivid];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Standard => "standard",
            Self::Vivid => "vivid",
        }
    }

    pub(crate) fn from_key(key: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|gain| Some(gain.key()) == key)
            .unwrap_or_default()
    }

    pub(crate) fn multiplier(self) -> f32 {
        match self {
            Self::Soft => 0.55,
            Self::Standard => 0.8,
            Self::Vivid => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceGlowPhase {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VoiceGlowPrefs {
    pub style: VoiceGlowStyle,
    pub gain: Gain,
    pub show_listening_thinking: bool,
}

impl Default for VoiceGlowPrefs {
    fn default() -> Self {
        Self {
            style: VoiceGlowStyle::default(),
            gain: Gain::default(),
            show_listening_thinking: true,
        }
    }
}

impl VoiceGlowPrefs {
    /// 经典只在回复时出现；关闭永不出现；新风格在开关关闭时聆听/思考折叠为空闲。
    pub(crate) fn effective_phase(self, phase: VoiceGlowPhase) -> VoiceGlowPhase {
        use VoiceGlowPhase::*;
        match (self.style, phase) {
            (VoiceGlowStyle::Off, _) => Idle,
            (VoiceGlowStyle::Classic, Listening | Thinking) => Idle,
            (_, Listening | Thinking) if !self.show_listening_thinking => Idle,
            _ => phase,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_glow_tests.rs"]
mod tests;
