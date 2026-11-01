//! 磁流体：卡片外缘一层被磁场牵动的液态金属膜。思考时表面蠕动、竖起小尖刺；
//! 工具调用时尖刺朝标题栏集中；等你时汇成底部一颗随心跳鼓动的液滴；
//! 完成时尖刺退去、镜面高光绕一圈；失败时整体下垂滴落。

use super::geometry::{
    ease_out_cubic, gaussian, hash, heartbeat, ring_distance, CardFrame, EdgePoint,
};
use super::palette::{mix_rgb, with_alpha_f};
use super::state::Targets;
use super::{RunFill, RunMotion, RunPrim, RunState};

/// [起伏幅度, 尖刺高度, 液滴, 下垂]
pub(super) const INITIAL: [f32; 4] = [0.8, 0.0, 0.0, 0.0];
pub(crate) const SAMPLES: usize = 240;
const SPIKES: usize = 16;
const SPIKE_STACK_CAP: f32 = 1.2;
/// 膜离卡片边缘的最远距离（缩放前像素），小于 `RunStyle::Ferrofluid.margin()`。
pub(crate) const MAX_REACH: f32 = 52.0;
/// 滴落长度上限：底边基础起伏 + 滴落 + 肩部仍落在 24px 布局间距内。
pub(crate) const MAX_DRIP: f32 = 16.0;

pub(super) fn targets(motion: &RunMotion) -> Targets {
    let state = motion.input.state;
    let amp = match state {
        RunState::Ready => 0.8,
        RunState::Thinking => 4.6,
        RunState::Tool => 4.2,
        RunState::Approval | RunState::Failed => 1.6,
        RunState::Done => 2.4,
    };
    let spike = match state {
        RunState::Thinking => 9.0,
        RunState::Tool => 8.0,
        _ => 0.0,
    };
    let on = |active: bool| if active { 1.0 } else { 0.0 };
    Targets {
        values: [
            amp,
            spike,
            on(state == RunState::Approval),
            on(state == RunState::Failed),
        ],
        taus: [260.0, 200.0, 320.0, 450.0],
        phase_speed: 0.0,
    }
}

/// 膜外轮廓上每个采样点离卡片边缘的距离（缩放前像素）。
pub(crate) fn offsets(motion: &RunMotion, frame: &CardFrame, base: &[EdgePoint]) -> Vec<f32> {
    let t = motion.time;
    let el = motion.since_change;
    let [amp, spike, drop, sag] = motion.values;
    let bottom = frame.bottom_center_u();
    let tool = motion.input.state == RunState::Tool;
    let ping = (1.0 - motion.since_ping / 280.0).max(0.0);
    let tau = std::f32::consts::TAU;
    base.iter()
        .enumerate()
        .map(|(i, p)| {
            let u = i as f32 / base.len() as f32;
            let mut d = 2.4
                + amp * (0.6 + 0.4 * (u * tau * 4.0 - t * 0.0016).sin())
                + amp * 0.5 * (u * tau * 11.0 + t * 0.0023).sin();
            let mut spikes = 0.0;
            for k in 0..SPIKES {
                let kf = k as f32;
                let mut height =
                    0.35 + 0.65 * (0.5 + 0.5 * (t * 0.004 * (1.0 + hash(kf)) + kf * 1.7).sin());
                if tool {
                    height *= if p.ny < -0.5 { 1.2 } else { 0.25 } * (1.0 + 0.5 * ping);
                }
                spikes += gaussian(ring_distance(u, hash(kf + 3.0)), 0.0055) * height;
            }
            // 相邻尖刺叠加会翻倍，封顶让外扩留在 24px 布局间距内。
            d += spike * spikes.min(SPIKE_STACK_CAP);
            let db = ring_distance(u, bottom);
            d = d * (1.0 - 0.55 * drop) + drop * (10.0 + 7.0 * heartbeat(t)) * gaussian(db, 0.045);
            if sag > 0.0 {
                if p.ny > 0.5 {
                    for k in 0..3 {
                        let kf = k as f32;
                        let uk = bottom + (kf - 1.0) * 0.09 + (hash(kf) - 0.5) * 0.03;
                        let drip = (el * 0.035 * (0.5 + hash(kf + 8.0))).min(MAX_DRIP);
                        d += sag * drip * gaussian(u - uk, 0.007)
                            + sag * 3.0 * gaussian(u - uk, 0.03);
                    }
                }
                if p.ny < 0.0 {
                    d *= 1.0 - 0.65 * sag;
                }
            }
            d.clamp(0.0, MAX_REACH)
        })
        .collect()
}

pub(super) fn build(motion: &RunMotion, frame: &CardFrame) -> Vec<RunPrim> {
    let base: Vec<EdgePoint> = (0..SAMPLES)
        .map(|i| frame.edge(i as f32 / SAMPLES as f32))
        .collect();
    let outer: Vec<[f32; 2]> = offsets(motion, frame, &base)
        .iter()
        .zip(&base)
        .map(|(d, p)| [p.x + p.nx * d * frame.zoom, p.y + p.ny * d * frame.zoom])
        .collect();
    let card: Vec<[f32; 2]> = base.iter().map(|p| [p.x, p.y]).collect();
    let color = motion.color;
    let body = mix_rgb(color, [6.0, 6.0, 12.0], 0.8);
    let level = motion.level.clamp(0.0, 1.2);
    let stroke =
        |width: f32, alpha: f32, points: Vec<[f32; 2]>, closed: bool, tint| RunPrim::Stroke {
            points,
            closed,
            width: width * frame.zoom,
            color: with_alpha_f(tint, alpha),
        };
    let mut prims = vec![
        RunPrim::Fill {
            contours: vec![outer.clone(), card],
            fill: RunFill::Vertical {
                top: with_alpha_f(mix_rgb(body, [255.0; 3], 0.16), 0.96),
                bottom: with_alpha_f(mix_rgb(body, [0.0; 3], 0.5), 0.96),
            },
        },
        stroke(6.0, 0.08 + 0.12 * level, outer.clone(), true, color),
        stroke(1.1, 0.3 + 0.55 * level, outer.clone(), true, color),
    ];
    highlight(
        &mut prims,
        &outer,
        (motion.time * 0.000_12).rem_euclid(1.0),
        0.07,
        0.45,
        frame.zoom,
    );
    let el = motion.since_change;
    if motion.input.state == RunState::Done && el < 1000.0 {
        let position = (ease_out_cubic(el / 1000.0) + 0.05).rem_euclid(1.0);
        highlight(
            &mut prims,
            &outer,
            position,
            0.12,
            0.95 * (1.0 - el / 1000.0),
            frame.zoom,
        );
    }
    if motion.input.state == RunState::Done && !motion.input.viewed {
        let alpha = 0.25 + 0.15 * (motion.time * 0.002).sin();
        prims.push(stroke(2.0, alpha, outer, true, color));
    }
    prims
}

/// 镜面高光：沿外轮廓一段白色描边，中心最亮，分 8 段递减。
fn highlight(
    prims: &mut Vec<RunPrim>,
    outer: &[[f32; 2]],
    center: f32,
    half: f32,
    alpha: f32,
    zoom: f32,
) {
    const PARTS: usize = 8;
    let n = outer.len();
    if alpha < 0.01 || n < 2 {
        return;
    }
    for part in 0..PARTS {
        let from = center - half + 2.0 * half * part as f32 / PARTS as f32;
        let to = from + 2.0 * half / PARTS as f32;
        let mid = (from + to) * 0.5;
        let a = alpha * (1.0 - (mid - center).abs() / half);
        let start = (from * n as f32).floor() as i64;
        let end = (to * n as f32).ceil() as i64;
        let points: Vec<[f32; 2]> = (start..=end)
            .map(|i| outer[i.rem_euclid(n as i64) as usize])
            .collect();
        prims.push(RunPrim::Stroke {
            points,
            closed: false,
            width: 1.6 * zoom,
            color: with_alpha_f([255.0; 3], a),
        });
    }
}
