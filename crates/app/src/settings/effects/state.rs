//! 特效设置的缓存与纯逻辑：谁生效、怎么改。渲染与派发只读这里的缓存，不碰 SQLite。

use crate::motion::run_style::RunStyle;
use crate::motion::StyleSelection;
use storage::DispatchEffectPreference as Pref;

/// 设置页网格的顺序；“random”不是具体款式，放在最后。
pub(crate) const GRID_STYLES: [&str; 8] = [
    "meteor",
    "plane",
    "ribbon",
    "firefly",
    "fireworks",
    "lightning",
    "vortex",
    "random",
];
const DEFAULT_STYLE: &str = "meteor";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    Voice,
    Idea,
    Handoff,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PlaybackSpeed {
    Relaxed,
    #[default]
    Standard,
    Brisk,
}

impl PlaybackSpeed {
    pub(crate) fn multiplier(self) -> f32 {
        match self {
            Self::Relaxed => 1.33,
            Self::Standard => 1.0,
            Self::Brisk => 0.75,
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Standard => "standard",
            Self::Brisk => "brisk",
        }
    }

    pub(crate) fn from_key(key: Option<&str>) -> Self {
        match key {
            Some("relaxed") => Self::Relaxed,
            Some("brisk") => Self::Brisk,
            _ => Self::Standard,
        }
    }
}

pub(crate) const KEY_DISPATCH: &str = "effects.dispatch.v1";
pub(crate) const KEY_SPEED: &str = "effects.speed";
/// 卡片运行特效风格：全局偏好，所有项目共用。
pub(crate) const KEY_RUN_STYLE: &str = "effects.run_style";

#[derive(Clone, Debug)]
pub(crate) struct EffectsSettingsState {
    pub global: Option<Pref>,
    pub speed: PlaybackSpeed,
    /// Prevent an initial background load from replacing a newer user edit.
    pub generation: u64,
    pub speed_generation: u64,
    pub status: Option<&'static str>,
    pub busy: bool,
    /// 最近一次修改，供“撤销”恢复。
    pub undo: Option<Option<Pref>>,
    pub run_style: RunStyle,
    /// 运行特效分区自己的状态行与撤销值，和派发特效互不干扰。
    pub run_status: Option<String>,
    pub run_undo: Option<RunStyle>,
    /// 最近一次确认写入数据库的风格与其序号；`run_generation` 是最新一次保存的序号。
    pub run_saved: RunStyle,
    pub run_saved_generation: u64,
    pub run_generation: u64,
}

impl Default for EffectsSettingsState {
    fn default() -> Self {
        Self {
            global: None,
            speed: PlaybackSpeed::default(),
            generation: 0,
            speed_generation: 0,
            status: None,
            busy: false,
            undo: None,
            run_style: RunStyle::default(),
            run_status: None,
            run_undo: None,
            run_saved: RunStyle::default(),
            run_saved_generation: 0,
            run_generation: 0,
        }
    }
}

impl gpui::Global for EffectsSettingsState {}

impl EffectsSettingsState {
    pub(crate) fn selection(&self) -> StyleSelection {
        self.global
            .as_ref()
            .map(StyleSelection::from_preference)
            .unwrap_or_default()
    }

    pub(crate) fn selected_style(&self) -> &str {
        self.global
            .as_ref()
            .map(|pref| pref.style.as_str())
            .filter(|style| GRID_STYLES.contains(style))
            .unwrap_or(DEFAULT_STYLE)
    }

    fn base(&self) -> Pref {
        self.global.clone().unwrap_or_else(|| Pref {
            style: DEFAULT_STYLE.into(),
            voice: None,
            idea: None,
            handoff: None,
        })
    }

    pub(crate) fn picked(&self, style: &str) -> Pref {
        Pref {
            style: style.into(),
            ..self.base()
        }
    }

    pub(crate) fn with_source(&self, source: Source, value: Option<&str>) -> Pref {
        let mut pref = self.base();
        let slot = match source {
            Source::Voice => &mut pref.voice,
            Source::Idea => &mut pref.idea,
            Source::Handoff => &mut pref.handoff,
        };
        *slot = value.map(str::to_owned);
        pref
    }
}
