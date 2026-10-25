//! 灵光：卡片边缘一圈光谱柔光，颜色即状态。
//!
//! GPUI 没有模糊与加法混合：每段用四条沿法线排开的半透明色带叠出柔光，
//! 相邻段共享边界顶点，转角处不留缝。色带一半在卡片内，即原型的“向内渗光”。

use super::geometry::{gaussian, hash, heartbeat, ring_distance, CardFrame};
use super::palette::{spectrum_at, with_alpha_f};
use super::state::Targets;
use super::{RunFill, RunMotion, RunPrim, RunState};

const INTENSITY: usize = 0;
const WIDTH: usize = 1;
/// [强度, 光带宽度系数, -, -]
pub(super) const INITIAL: [f32; 4] = [0.0, 1.0, 0.0, 0.0];
/// 24 段 × 4 色带 = 96 条路径；36 段时实测单卡绘制均值 0.44ms，超过 0.4ms 预算。
pub(crate) const SEGMENTS: usize = 24;
const STEPS_PER_SEGMENT: usize = 6;
/// (内侧偏移, 外侧偏移, 透明度)，单位为缩放前像素；负数在卡片内。
const BANDS: [(f32, f32, f32); 4] = [
    (-6.0, 14.0, 0.10),
    (-3.0, 7.0, 0.18),
    (-1.5, 3.0, 0.28),
    (-0.65, 0.65, 0.9),
];

pub(super) fn targets(motion: &RunMotion) -> Targets {
    let el = motion.since_change;
    let intensity = match motion.input.state {
        RunState::Ready => 0.0,
        RunState::Thinking => 0.8,
        RunState::Tool => 1.0,
        RunState::Approval => 0.75,
        RunState::Done if motion.input.viewed => 0.0,
        RunState::Done if el < 700.0 => 1.2,
        RunState::Done => 0.55,
        RunState::Failed if el < 260.0 => 1.1,
        RunState::Failed => 0.4,
    };
    let speed = match motion.input.state {
        RunState::Thinking => 0.000_07,
        RunState::Tool => 0.000_18,
        RunState::Approval => 0.000_03,
        RunState::Done => 0.000_04,
        RunState::Ready | RunState::Failed => 0.0,
    };
    let width = if motion.input.state == RunState::Failed {
        0.45
    } else {
        1.0
    };
    Targets {
        values: [intensity, width, 0.0, 0.0],
        taus: [200.0, 300.0, 1.0, 1.0],
        phase_speed: speed,
    }
}

pub(super) fn build(motion: &RunMotion, frame: &CardFrame) -> Vec<RunPrim> {
    let mut intensity = motion.values[INTENSITY];
    let t = motion.time;
    let el = motion.since_change;
    match motion.input.state {
        RunState::Approval => intensity *= 0.55 + 0.45 * heartbeat(t),
        // 失败：先不规则闪两三下，再收成暗红细环。
        RunState::Failed if el < 700.0 && hash((el / 60.0).floor()) > 0.6 => intensity *= 0.3,
        _ => {}
    }
    if intensity < 0.01 {
        return Vec::new();
    }
    let width = motion.values[WIDTH];
    // 工具芯片在标题栏右侧：从上边右端往回 24px 处出发。
    let tool_origin =
        ((frame.w - 2.0 * frame.radius - 24.0 * frame.zoom).max(0.0)) / frame.perimeter().max(1.0);
    let ping = motion.since_ping;
    let samples = SEGMENTS * STEPS_PER_SEGMENT;
    let points: Vec<_> = (0..=samples)
        .map(|i| frame.edge(i as f32 / samples as f32))
        .collect();
    let mut prims = Vec::with_capacity(SEGMENTS * BANDS.len());
    for segment in 0..SEGMENTS {
        let u = (segment as f32 + 0.5) / SEGMENTS as f32;
        let mut k = 0.55
            + 0.45
                * (u * std::f32::consts::TAU * 3.0 + t * 0.0011).sin()
                * (u * std::f32::consts::TAU * 2.0 - t * 0.0007).sin();
        if motion.input.state == RunState::Tool {
            let d = ring_distance(u, tool_origin + ping / 650.0);
            k += 1.6 * gaussian(d, 0.035) * (1.0 - ping / 900.0).max(0.0);
        }
        if motion.input.state == RunState::Done && el < 900.0 {
            k += 1.4 * gaussian(ring_distance(u, el / 900.0), 0.05);
        }
        let color = spectrum_at(&motion.spectrum, u + motion.phase);
        let run = &points[segment * STEPS_PER_SEGMENT..=(segment + 1) * STEPS_PER_SEGMENT];
        for (inner, outer, alpha) in BANDS {
            let a = (alpha * intensity * k).clamp(0.0, 1.0);
            if a < 0.004 {
                continue;
            }
            let (inner, outer) = (inner * width * frame.zoom, outer * width * frame.zoom);
            let mut contour: Vec<[f32; 2]> = run
                .iter()
                .map(|p| [p.x + p.nx * outer, p.y + p.ny * outer])
                .collect();
            contour.extend(
                run.iter()
                    .rev()
                    .map(|p| [p.x + p.nx * inner, p.y + p.ny * inner]),
            );
            prims.push(RunPrim::Fill {
                contours: vec![contour],
                fill: RunFill::Solid(with_alpha_f(color, a)),
            });
        }
    }
    prims
}
