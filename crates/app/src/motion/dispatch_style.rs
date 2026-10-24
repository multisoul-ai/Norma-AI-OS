//! 派发特效款式：纯数据与纯函数。时长取自 `docs/prototypes/dispatch-effects` 的时间轴，
//! 绘制在 `effects::dispatch_receipt`，存储层只保存下面的字符串 id。

use super::DispatchVisualOrigin;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) enum DispatchEffectStyle {
    /// 语音与想法清单的默认款式。
    #[default]
    Meteor,
    /// Agent 间交接的默认款式（现有离子画法）。
    Ion,
    Plane,
    Ribbon,
    Firefly,
    Fireworks,
    Lightning,
    Vortex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StyleTiming {
    pub flight: Duration,
    pub impact: Duration,
}

/// “随机”只从这些有完整飞行动画的款式里选。
const RANDOM_POOL: [DispatchEffectStyle; 7] = [
    DispatchEffectStyle::Meteor,
    DispatchEffectStyle::Plane,
    DispatchEffectStyle::Ribbon,
    DispatchEffectStyle::Firefly,
    DispatchEffectStyle::Fireworks,
    DispatchEffectStyle::Lightning,
    DispatchEffectStyle::Vortex,
];

impl DispatchEffectStyle {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Meteor => "meteor",
            Self::Ion => "ion",
            Self::Plane => "plane",
            Self::Ribbon => "ribbon",
            Self::Firefly => "firefly",
            Self::Fireworks => "fireworks",
            Self::Lightning => "lightning",
            Self::Vortex => "vortex",
        }
    }

    /// 未显式指定款式时沿用现状：语音与想法是流星，Agent 交接是离子。
    pub(crate) fn default_for(origin: DispatchVisualOrigin) -> Self {
        match origin {
            DispatchVisualOrigin::AgentNode(_) => Self::Ion,
            DispatchVisualOrigin::CommandDock | DispatchVisualOrigin::IdeaRow { .. } => {
                Self::Meteor
            }
        }
    }

    /// 未知 id（含 "random"，它是偏好而不是可绘制款式）返回 `None`，由调用方回落到默认。
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        [Self::Ion]
            .into_iter()
            .chain(RANDOM_POOL)
            .find(|style| style.id() == id)
    }

    /// 同一个 `task_ref` 永远得到同一款，保证去重、重放与测试可复现。
    pub(crate) fn random_for(task_ref: &str) -> Self {
        let hash = task_ref.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        RANDOM_POOL[(hash >> 33) as usize % RANDOM_POOL.len()]
    }

    fn base_millis(self) -> (u64, u64) {
        match self {
            Self::Meteor => (900, 430),
            Self::Ion => (480, 430),
            Self::Plane => (1020, 600),
            Self::Ribbon => (660, 700),
            Self::Firefly => (920, 660),
            Self::Fireworks => (600, 1300),
            Self::Lightning => (170, 590),
            Self::Vortex => (1420, 340),
        }
    }

    /// `speed` 是时长倍率（1.0 标准，大于 1 更舒缓）。
    pub(crate) fn timing(self, speed: f32) -> StyleTiming {
        let (flight, impact) = self.base_millis();
        let scale = |ms: u64| (ms as f32 * speed.clamp(0.25, 4.0)).round() as u64;
        StyleTiming {
            flight: Duration::from_millis(scale(flight)),
            impact: Duration::from_millis(scale(impact)),
        }
    }
}

/// 偏好里的一项选择：具体款式，或派发时随机。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StyleChoice {
    Style(DispatchEffectStyle),
    Random,
}

impl StyleChoice {
    /// 未知 id 返回 `None`，等同未设置。
    fn parse(id: &str) -> Option<Self> {
        if id == "random" {
            Some(Self::Random)
        } else {
            DispatchEffectStyle::from_id(id).map(Self::Style)
        }
    }

    fn resolve(self, task_ref: &str) -> DispatchEffectStyle {
        match self {
            Self::Style(style) => style,
            Self::Random => DispatchEffectStyle::random_for(task_ref),
        }
    }
}

/// 某个项目当前生效的选择（项目覆盖 → 工作空间 → 默认已在上游合并）。
/// 来源覆盖为 `None` 表示跟随 `base`；`base` 为 `None`（没有任何设置）时用各来源的默认款式。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StyleSelection {
    base: Option<StyleChoice>,
    voice: Option<StyleChoice>,
    idea: Option<StyleChoice>,
    handoff: Option<StyleChoice>,
}

impl StyleSelection {
    pub(crate) fn from_preference(preference: &storage::DispatchEffectPreference) -> Self {
        let parse = |id: &Option<String>| id.as_deref().and_then(StyleChoice::parse);
        Self {
            base: StyleChoice::parse(&preference.style),
            voice: parse(&preference.voice),
            idea: parse(&preference.idea),
            handoff: parse(&preference.handoff),
        }
    }

    pub(crate) fn style_for(
        self,
        origin: DispatchVisualOrigin,
        task_ref: &str,
    ) -> DispatchEffectStyle {
        let source = match origin {
            DispatchVisualOrigin::CommandDock => self.voice,
            DispatchVisualOrigin::IdeaRow { .. } => self.idea,
            DispatchVisualOrigin::AgentNode(_) => self.handoff,
        };
        source.or(self.base).map_or_else(
            || DispatchEffectStyle::default_for(origin),
            |choice| choice.resolve(task_ref),
        )
    }
}
