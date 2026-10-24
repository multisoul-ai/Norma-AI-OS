use super::super::build_paint_plan;
use super::*;
use crate::motion::{
    DispatchEffectPresentation, DispatchImpactParticle, DispatchReceiptVisualStatus,
    DispatchVisualOrigin,
};
use gpui::{point, px, Bounds, Size};

const STYLES: [DispatchEffectStyle; 6] = [
    DispatchEffectStyle::Plane,
    DispatchEffectStyle::Ribbon,
    DispatchEffectStyle::Firefly,
    DispatchEffectStyle::Fireworks,
    DispatchEffectStyle::Lightning,
    DispatchEffectStyle::Vortex,
];
/// 单个特效每帧图元（paths + quads）上限；超出说明要砍尾迹段数或粒子数。
const BUDGET: usize = 450;

fn frame(style: DispatchEffectStyle, t_ms: f32, scale: f32, seed: u64) -> DispatchEffectFrame {
    let timing = style.timing(1.0);
    let (flight, impact) = (
        timing.flight.as_secs_f32() * 1000.0,
        timing.impact.as_secs_f32() * 1000.0,
    );
    let (phase, progress) = if t_ms < flight {
        (DispatchEffectPhase::Flight, t_ms / flight)
    } else {
        (DispatchEffectPhase::Impact, (t_ms - flight) / impact)
    };
    DispatchEffectFrame {
        origin: point(px(640.0), px(752.0)),
        origin_bounds: None,
        target: point(px(300.0), px(220.0)),
        target_bounds: Bounds {
            origin: point(px(180.0), px(140.0)),
            size: Size {
                width: px(240.0),
                height: px(160.0),
            },
        },
        scale,
        presentation: DispatchEffectPresentation {
            style,
            seed,
            origin: DispatchVisualOrigin::CommandDock,
            target_node_id: 7,
            status: DispatchReceiptVisualStatus::Submitted,
            phase,
            progress: progress.clamp(0.0, 1.0),
            particles: [DispatchImpactParticle::default(); 28],
            particle_count: 0,
        },
    }
}

fn total_ms(style: DispatchEffectStyle) -> f32 {
    let timing = style.timing(1.0);
    (timing.flight + timing.impact).as_secs_f32() * 1000.0
}

fn counts(style: DispatchEffectStyle, t: f32, scale: f32, seed: u64) -> (usize, usize) {
    let plan = build_paint_plan(
        Bounds {
            origin: point(px(0.0), px(0.0)),
            size: Size {
                width: px(1200.0),
                height: px(800.0),
            },
        },
        &[frame(style, t, scale, seed)],
        Theme::default(),
    );
    (plan.paths.len(), plan.quads.len())
}

/// 每款在整条时间轴上：不 panic、不超预算、同输入同输出、中途确实有画面。
///
/// 数据构造：6 款 × 时间轴每 20ms 一个采样（最长 1900ms → 95 个）；种子 11。
/// 预期结果：
///   - 正断言：每个采样 paths+quads ≤ 450；两次计算图元数量完全一致；时间轴中点附近至少有 1 个图元。
///   - 负断言：时间轴末尾之后一帧（progress=1）不会比预算更多；种子不同的两次渲染形状可以不同但同样不超预算。
#[test]
fn every_style_stays_in_budget_and_is_deterministic() {
    for style in STYLES {
        let total = total_ms(style);
        let mut max_seen = 0;
        let mut drawn_midway = false;
        let mut t = 0.0;
        while t <= total {
            let a = counts(style, t, 1.0, 11);
            assert_eq!(a, counts(style, t, 1.0, 11), "{style:?} t={t} 应确定");
            let sum = a.0 + a.1;
            assert!(
                sum <= BUDGET,
                "{style:?} t={t} 图元 {sum} 超过预算 {BUDGET}"
            );
            max_seen = max_seen.max(sum);
            if (total * 0.3..total * 0.6).contains(&t) && sum > 0 {
                drawn_midway = true;
            }
            assert!(
                counts(style, t, 1.0, 99).0 + counts(style, t, 1.0, 99).1 <= BUDGET,
                "{style:?} 另一种子也应在预算内"
            );
            t += 20.0;
        }
        assert!(drawn_midway, "{style:?} 时间轴中段应有画面");
        assert!(
            max_seen > 5,
            "{style:?} 至少应画出多个图元，实际最多 {max_seen}"
        );
    }
}

/// 缩放只改尺寸不改结构：缩略图（0.55）与整图（1.0）的图元数量一致。
///
/// 数据构造：6 款在各自时间轴 45% 处各取一帧，scale 分别为 1.0 与 0.55。
/// 预期结果：
///   - 正断言：两种 scale 下 paths 与 quads 数量相同。
///   - 负断言：极小 scale（0.05）也不 panic。
#[test]
fn scale_keeps_structure() {
    for style in STYLES {
        let t = total_ms(style) * 0.45;
        assert_eq!(
            counts(style, t, 1.0, 5),
            counts(style, t, 0.55, 5),
            "{style:?} 缩放不应改变图元数量"
        );
        let _ = counts(style, t, 0.05, 5);
    }
}

/// 非本模块绘制的款式返回 false，交给旧实现。
///
/// 数据构造：Meteor / Ion 各一帧。
/// 预期结果：
///   - 正断言：2 款都返回 false 且不往 plan 里写图元。
///   - 负断言：6 款新款式都返回 true。
#[test]
fn legacy_styles_are_not_handled_here() {
    for style in [DispatchEffectStyle::Meteor, DispatchEffectStyle::Ion] {
        let mut plan = DispatchPaintPlan::empty_for_test();
        assert!(
            !paint_styled(&frame(style, 300.0, 1.0, 1), Theme::default(), &mut plan),
            "{style:?}"
        );
        assert_eq!(plan.paths.len() + plan.quads.len(), 0);
    }
    for style in STYLES {
        let mut plan = DispatchPaintPlan::empty_for_test();
        assert!(
            paint_styled(
                &frame(style, total_ms(style) * 0.5, 1.0, 1),
                Theme::default(),
                &mut plan
            ),
            "{style:?}"
        );
    }
}

/// 缩略图里的卡片可能只有几十像素宽：任何款式、任何时刻都不能因为区间反转而 panic。
///
/// 数据构造：卡片宽 48.6×27.4（设置页网格里的实际大小），scale=0.27；6 款 × 每 20ms 采样。
/// 预期结果：
///   - 正断言：所有采样都能生成图元，数量仍在预算内。
///   - 负断言：不触发 `f32::clamp` 的 min>max panic（回归：声纹在小卡片上崩溃）。
#[test]
fn tiny_preview_geometry_never_panics() {
    for style in STYLES {
        let total = total_ms(style);
        let mut t = 0.0;
        while t <= total {
            let mut f = frame(style, t, 0.27, 3);
            f.target_bounds = Bounds {
                origin: point(px(849.8), px(40.0)),
                size: Size {
                    width: px(48.6),
                    height: px(27.4),
                },
            };
            f.origin = point(px(814.0), px(115.0));
            let plan = build_paint_plan(
                Bounds {
                    origin: point(px(780.0), px(0.0)),
                    size: Size {
                        width: px(130.0),
                        height: px(73.0),
                    },
                },
                &[f],
                Theme::default(),
            );
            assert!(
                plan.paths.len() + plan.quads.len() <= BUDGET,
                "{style:?} t={t}"
            );
            t += 20.0;
        }
    }
}
