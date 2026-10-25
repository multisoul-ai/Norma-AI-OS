//! Agent 卡片运行特效：风格选择、状态平滑与各风格的绘制图元。
//!
//! 全部是不依赖 GPUI 的纯数据与纯函数，GPUI 绘制见 `cards/run_effect`。
//! 视觉以 `docs/prototypes/agent-card-run-effects` 为准。

mod aura;
mod ferrofluid;
pub(crate) mod geometry;
mod palette;
mod state;
mod underline;

pub(crate) use geometry::CardFrame;
pub(crate) use palette::{with_alpha_f, RunPalette};
pub(crate) use state::{run_input, RunInput, RunMotion, RunState};

/// 用户可选的运行特效风格。`Classic` 是现有的光蛇、呼吸与完成柔光，不走本模块绘制。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) enum RunStyle {
    #[default]
    Classic,
    Underline,
    Aura,
    Ferrofluid,
}

impl RunStyle {
    pub(crate) const ALL: [RunStyle; 4] = [
        RunStyle::Classic,
        RunStyle::Underline,
        RunStyle::Aura,
        RunStyle::Ferrofluid,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Underline => "underline",
            Self::Aura => "aura",
            Self::Ferrofluid => "ferrofluid",
        }
    }

    /// 未知值与空值回落到经典，兼容将来版本写入的新风格。
    pub(crate) fn from_id(id: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|style| Some(style.id()) == id)
            .unwrap_or_default()
    }

    /// 外缘类风格画在所有卡片之下，外扩部分不会盖住相邻卡片（布局间距只有 8px）。
    pub(crate) fn draws_under_cards(self) -> bool {
        matches!(self, Self::Aura | Self::Ferrofluid)
    }

    /// 绘制区域向卡片外扩的距离（缩放前像素）：灵光的外缘柔光、磁流体的液滴与滴落。
    pub(crate) fn margin(self) -> f32 {
        match self {
            Self::Classic | Self::Underline => 0.0,
            Self::Aura => 18.0,
            Self::Ferrofluid => 56.0,
        }
    }
}

/// 绘制图元；坐标为绘制区域内的像素。
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RunPrim {
    /// 折线描边。
    Stroke {
        points: Vec<[f32; 2]>,
        closed: bool,
        width: f32,
        color: u32,
    },
    /// 多个闭合轮廓按奇偶规则填充，用来画环形膜。
    Fill {
        contours: Vec<Vec<[f32; 2]>>,
        fill: RunFill,
    },
    /// 横向渐变的矩形条：一道光两端的渐隐。
    Bar {
        x0: f32,
        x1: f32,
        y: f32,
        height: f32,
        left: u32,
        right: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RunFill {
    Solid(u32),
    Vertical { top: u32, bottom: u32 },
}

/// 单卡每帧图元上限：超过即说明某个风格的分段失控，测试会拦住。
pub(crate) const MAX_PRIMS: usize = 160;

/// 按风格生成当前帧的图元。经典返回空，由现有代码绘制。
pub(crate) fn build(
    style: RunStyle,
    motion: &RunMotion,
    frame: &CardFrame,
    reduce_motion: bool,
) -> Vec<RunPrim> {
    if reduce_motion && matches!(style, RunStyle::Aura | RunStyle::Ferrofluid) {
        return reduced_bar(motion, frame);
    }
    let prims = match style {
        RunStyle::Classic => Vec::new(),
        RunStyle::Underline => underline::build(motion, frame, reduce_motion),
        RunStyle::Aura => aura::build(motion, frame),
        RunStyle::Ferrofluid => ferrofluid::build(motion, frame),
    };
    debug_assert!(prims.len() <= MAX_PRIMS, "{style:?} 图元数超预算");
    prims
}

/// 减少动态效果：卡片底部 3px 状态色细条，没有位移和节奏。
fn reduced_bar(motion: &RunMotion, frame: &CardFrame) -> Vec<RunPrim> {
    let alpha = motion.level.clamp(0.0, 1.0);
    if alpha < 0.01 {
        return Vec::new();
    }
    let color = palette::with_alpha_f(motion.color, 0.8 * alpha);
    let height = 3.0 * frame.zoom;
    let inset = frame.radius;
    vec![RunPrim::Bar {
        x0: frame.x + inset,
        x1: frame.x + frame.w - inset,
        y: frame.y + frame.h - height,
        height,
        left: color,
        right: color,
    }]
}

#[cfg(test)]
#[path = "../../../tests/unit/run_style_tests.rs"]
mod tests;
