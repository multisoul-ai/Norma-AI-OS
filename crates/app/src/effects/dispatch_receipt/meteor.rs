//! 想法流星：白色光点，沿一条轻微弯曲的弧线逐步加速飞向卡片中央，尾迹随速度拉长。
//!
//! 对照原型 `flyMeteor`：
//! - 进度用 easeInOutCubic（先慢后快再收），所以是“逐步加速”，临近目标时收速。
//! - 尾迹是一串由细到粗、由冷白到纯白的小段，越靠近头部越亮越粗（宽度 1 + 6.5·life²）；
//!   速度越快，尾迹覆盖的行程越长。
//! - 头部是纯白亮点加两层冷白光晕；尾迹两侧散落几粒细小的火花。
//! 撞击阶段的白色光环由 `impact.rs` 绘制。

#[cfg(test)]
#[path = "../../../tests/unit/dispatch_idea_meteor_tests.rs"]
mod tests;

use super::geometry::FlightCurve;
use super::primitives::{centered_bounds, round_quad, with_alpha};
use super::DispatchPaintPlan;
use gpui::{point, px, rgba, PathBuilder, Pixels, Point};

const TRAIL_SEGMENTS: usize = 28;
const SPARKS: usize = 18;
/// 尾焰覆盖的时间占整段飞行的比例（原型是 320ms / 约 760ms）。
const TAIL_SPAN: f32 = 0.42;

/// easeInOutCubic：前半段加速，后半段减速。
pub(super) fn ease_in_out_cubic(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
    }
}

/// 尾焰起点对应的行程比例。
fn tail_start(linear_progress: f32) -> f32 {
    ease_in_out_cubic((linear_progress - TAIL_SPAN).max(0.0))
}

pub(super) fn paint_idea_meteor(
    curve: &FlightCurve,
    linear_progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let travel = ease_in_out_cubic(linear_progress);
    if travel <= 0.0 {
        return;
    }
    // 尾焰是“最近一小段时间飞过的路径”：起步时短，加速后越拖越长，转弯处自然跟着弧线弯。
    let start = tail_start(linear_progress);
    paint_trail(curve, start, travel, 1.0, plan);
    paint_sparks(curve, start, travel, 1.0, plan);
    paint_head(curve.point_at_travel(travel), linear_progress, plan);
}

/// 撞击之后头部已经消失，尾焰继续向目标收拢并淡出，不会在落点处被硬生生截断。
pub(super) fn paint_idea_afterglow(
    curve: &FlightCurve,
    impact_progress: f32,
    plan: &mut DispatchPaintPlan,
) {
    let fade = 1.0 - impact_progress.clamp(0.0, 1.0);
    if fade <= 0.0 {
        return;
    }
    let tail_from = ease_in_out_cubic(1.0 - TAIL_SPAN);
    let start = tail_from + (1.0 - tail_from) * impact_progress.clamp(0.0, 1.0);
    paint_trail(curve, start, 1.0, fade, plan);
    paint_sparks(curve, start, 1.0, fade, plan);
}

fn paint_trail(curve: &FlightCurve, start: f32, end: f32, fade: f32, plan: &mut DispatchPaintPlan) {
    if end - start < 0.002 {
        return;
    }
    for index in 0..TRAIL_SEGMENTS {
        let from = start + (end - start) * index as f32 / TRAIL_SEGMENTS as f32;
        let to = start + (end - start) * (index + 1) as f32 / TRAIL_SEGMENTS as f32;
        let life = (index + 1) as f32 / TRAIL_SEGMENTS as f32;
        let width = 0.6 + 7.4 * life.powf(1.8);
        let tint = |cool: u32, hot: u32| {
            (cool as f32 + (hot as f32 - cool as f32) * life.powf(1.4)).round() as u32
        };
        // 尾端偏冷蓝，向头部渐变为纯白。
        let color = (tint(150, 255) << 24) | (tint(205, 255) << 16) | (tint(255, 255) << 8);
        // 由外到内三层：宽而淡的火焰外焰、中层辉光、白色内芯。
        for (scale, alpha) in [
            (5.0, 0.07 * life.powf(1.5)),
            (2.2, 0.26 * life),
            (1.0, 0.95 * life.powf(0.8)),
        ] {
            let mut builder = PathBuilder::stroke(px(width * scale));
            builder.move_to(curve.point_at_travel(from));
            builder.line_to(curve.point_at_travel(to));
            if let Ok(path) = builder.build() {
                plan.paths.push((
                    path,
                    rgba(with_alpha(color, (alpha * fade * 255.0) as u8)).into(),
                ));
            }
        }
    }
}

fn paint_sparks(
    curve: &FlightCurve,
    start: f32,
    end: f32,
    fade: f32,
    plan: &mut DispatchPaintPlan,
) {
    if end - start < 0.002 {
        return;
    }
    for index in 0..SPARKS {
        // along: 0 = 尾端，1 = 头部。火花越靠近尾端，离中轴越远、越暗。
        let along = (index as f32 + 0.5) / SPARKS as f32;
        let travel = start + (end - start) * along;
        let center = curve.point_at_travel(travel);
        let tangent = curve.tangent_at_travel(travel);
        let jitter = ((index as f32 * 12.9898).sin() * 43_758.547).fract().abs() - 0.5;
        let drift = ((index as f32 * 78.233).sin() * 12_345.679).fract().abs();
        let side = jitter * 2.0 * (3.0 + 15.0 * (1.0 - along));
        let back = drift * 6.0 * (1.0 - along);
        let at = point(
            center.x + px(-tangent.1 * side - tangent.0 * back),
            center.y + px(tangent.0 * side - tangent.1 * back),
        );
        let size = 0.9 + 1.6 * along * (0.5 + drift);
        plan.quads.push(round_quad(
            centered_bounds(at, size, size),
            size / 2.0,
            with_alpha(0xeaf6ffff, (0.8 * along.powf(0.7) * fade * 255.0) as u8),
        ));
    }
}

fn paint_head(head: Point<Pixels>, linear_progress: f32, plan: &mut DispatchPaintPlan) {
    // 接近目标时头部略微收小，与撞击光环衔接。
    let settle = 1.0 - 0.3 * ((linear_progress - 0.85) / 0.15).clamp(0.0, 1.0);
    for (size, alpha) in [(36.0, 0x1c), (22.0, 0x40), (12.0, 0x90)] {
        let size = size * settle;
        plan.quads.push(round_quad(
            centered_bounds(head, size, size),
            size / 2.0,
            with_alpha(0xdff1ffff, alpha),
        ));
    }
    let core = 5.5 * settle;
    plan.quads.push(round_quad(
        centered_bounds(head, core, core),
        core / 2.0,
        0xffffffff,
    ));
}
