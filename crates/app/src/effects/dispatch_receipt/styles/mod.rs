//! 派发特效的新款式。每款一个文件，都是 `StyleFrame` 的纯函数；
//! 流星与离子沿用旧实现（`meteor.rs` / `flight.rs`），脉冲走减少动态效果的绘制。

mod firefly;
mod fireworks;
mod lightning;
mod painter;
mod plane;
mod ribbon;
mod vortex;

use self::painter::{Painter, Palette, Rect, V};
use super::{DispatchEffectFrame, DispatchPaintPlan};
use crate::motion::{DispatchEffectPhase, DispatchEffectStyle};
use ui::Theme;

/// 一帧的绘制输入：`t` 是原型时间轴上的毫秒（飞行段 + 命中段连续计时），与速度倍率无关。
struct StyleFrame {
    t: f32,
    origin: V,
    card: Rect,
    seed: u64,
    pal: Palette,
}

/// 返回 `false` 表示该款式不由本模块绘制（流星、离子、脉冲）。
pub(super) fn paint_styled(
    frame: &DispatchEffectFrame,
    theme: Theme,
    plan: &mut DispatchPaintPlan,
) -> bool {
    let style = frame.presentation.style;
    let paint: fn(&StyleFrame, &mut Painter) = match style {
        DispatchEffectStyle::Plane => plane::paint,
        DispatchEffectStyle::Ribbon => ribbon::paint,
        DispatchEffectStyle::Firefly => firefly::paint,
        DispatchEffectStyle::Fireworks => fireworks::paint,
        DispatchEffectStyle::Lightning => lightning::paint,
        DispatchEffectStyle::Vortex => vortex::paint,
        DispatchEffectStyle::Meteor | DispatchEffectStyle::Ion => return false,
    };
    let timing = style.timing(1.0);
    let flight = timing.flight.as_secs_f32() * 1000.0;
    let impact = timing.impact.as_secs_f32() * 1000.0;
    let progress = frame.presentation.progress.clamp(0.0, 1.0);
    let t = match frame.presentation.phase {
        DispatchEffectPhase::Flight => progress * flight,
        DispatchEffectPhase::Impact => flight + progress * impact,
    };
    let styled = StyleFrame {
        t,
        origin: frame.origin.into(),
        card: frame.target_bounds.into(),
        seed: frame.presentation.seed,
        pal: Palette::for_theme(theme),
    };
    let mut painter = Painter {
        plan,
        scale: frame.scale,
    };
    paint(&styled, &mut painter);
    true
}

#[cfg(test)]
#[path = "../../../../tests/unit/dispatch_styles_tests.rs"]
mod tests;
