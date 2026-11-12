//! 语音光晕：风格、相位与偏好。纯逻辑，风格只产出图元，由 `app::voice_window_inset` 负责绘制。

pub(crate) mod preview;
pub(crate) mod scene;
pub(crate) mod styles;

/// 键：风格 / 强度 / 聆听与思考是否显示。全局偏好，所有项目共用。
pub(crate) const KEY_STYLE: &str = "effects.voice_glow.style";
pub(crate) const KEY_GAIN: &str = "effects.voice_glow.gain";
pub(crate) const KEY_PHASES: &str = "effects.voice_glow.phases";
/// 旧「说话光晕」开关，仅用于首次迁移。
pub(crate) const KEY_LEGACY_ENABLED: &str = "voice.window_inset_enabled";

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

/// 偏好对应的三个设置键值，供一次事务写入。
pub(crate) fn prefs_changes(prefs: VoiceGlowPrefs) -> Vec<(String, Option<String>)> {
    vec![
        (KEY_STYLE.to_owned(), Some(prefs.style.id().to_owned())),
        (KEY_GAIN.to_owned(), Some(prefs.gain.key().to_owned())),
        (
            KEY_PHASES.to_owned(),
            Some(prefs.show_listening_thinking.to_string()),
        ),
    ]
}

/// 读取偏好；风格键缺失时按旧开关迁移并写回。读库出错时不迁移、不写回：
/// 否则数据库暂时繁忙会把用户已选的风格覆盖成默认值。
pub(crate) fn load_prefs<E>(
    get: impl Fn(&str) -> Result<Option<String>, E>,
    mut set: impl FnMut(&str, &str) -> Result<(), E>,
) -> VoiceGlowPrefs {
    let read = |key: &str| get(key).ok().flatten();
    let style = match get(KEY_STYLE) {
        Ok(Some(id)) => VoiceGlowStyle::from_id(Some(&id)),
        Ok(None) => {
            let legacy = match get(KEY_LEGACY_ENABLED) {
                Ok(value) => {
                    value.and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                        "1" | "true" | "yes" | "on" => Some(true),
                        "0" | "false" | "no" | "off" => Some(false),
                        _ => None,
                    })
                }
                Err(_) => return VoiceGlowPrefs::default(),
            };
            let style = VoiceGlowStyle::migrate_legacy(legacy);
            if set(KEY_STYLE, style.id()).is_err() {
                tracing::warn!("voice glow style migration write failed");
            }
            style
        }
        Err(_) => return VoiceGlowPrefs::default(),
    };
    VoiceGlowPrefs {
        style,
        gain: Gain::from_key(read(KEY_GAIN).as_deref()),
        show_listening_thinking: read(KEY_PHASES).as_deref() != Some("false"),
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
