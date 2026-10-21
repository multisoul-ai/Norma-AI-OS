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

const TRAIL_SEGMENTS: usize = 14;
const SPARKS: usize = 8;

/// easeInOutCubic：前半段加速，后半段减速。
pub(super) fn ease_in_out_cubic(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
    }
}

/// easeInOutCubic 的导数（相对速度，峰值 3）。
fn speed(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    if p < 0.5 {
        12.0 * p * p
    } else {
        3.0 * (-2.0 * p + 2.0).powi(2)
    }
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
    // 尾迹长度随速度变化：起步和收尾很短，中段最长。
    let length = 0.035 + 0.085 * speed(linear_progress);
    let start = (travel - length).max(0.0);
    paint_trail(curve, start, travel, plan);
    paint_sparks(curve, start, travel, plan);
    paint_head(curve.point_at_travel(travel), linear_progress, plan);
}

fn paint_trail(curve: &FlightCurve, start: f32, end: f32, plan: &mut DispatchPaintPlan) {
    for index in 0..TRAIL_SEGMENTS {
        let from = start + (end - start) * index as f32 / TRAIL_SEGMENTS as f32;
        let to = start + (end - start) * (index + 1) as f32 / TRAIL_SEGMENTS as f32;
        let life = (index + 1) as f32 / TRAIL_SEGMENTS as f32;
        let width = 1.0 + 6.5 * life * life;
        let tint =
            |cool: u32, hot: u32| (cool as f32 + (hot as f32 - cool as f32) * life).round() as u32;
        let color = (tint(176, 255) << 24) | (tint(218, 255) << 16) | (tint(255, 255) << 8);
        // 外圈柔光 + 内芯。
        for (scale, alpha) in [(3.0, 0.14 * life), (1.0, 0.92 * life)] {
            let mut builder = PathBuilder::stroke(px(width * scale));
            builder.move_to(curve.point_at_travel(from));
            builder.line_to(curve.point_at_travel(to));
            if let Ok(path) = builder.build() {
                plan.paths
                    .push((path, rgba(with_alpha(color, (alpha * 255.0) as u8)).into()));
            }
        }
    }
}

fn paint_sparks(curve: &FlightCurve, start: f32, end: f32, plan: &mut DispatchPaintPlan) {
    for index in 0..SPARKS {
        let along = (index as f32 + 0.5) / SPARKS as f32;
        let center = curve.point_at_travel(start + (end - start) * along);
        let tangent = curve.tangent_at_travel(start + (end - start) * along);
        // 用下标做确定性的“随机”偏移，避免每帧闪烁。
        let jitter = ((index as f32 * 12.9898).sin() * 43_758.547).fract().abs() - 0.5;
        let side = jitter * 2.0 * (4.0 + 10.0 * (1.0 - along));
        let at = point(
            center.x + px(-tangent.1 * side),
            center.y + px(tangent.0 * side),
        );
        let size = 1.2 + 1.4 * along;
        plan.quads.push(round_quad(
            centered_bounds(at, size, size),
            size / 2.0,
            with_alpha(0xe6f4ffff, (0.75 * along * 255.0) as u8),
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
