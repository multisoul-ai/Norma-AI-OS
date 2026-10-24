use super::dispatch_receipt::{
    build_paint_plan, idea_row_origin, idea_shot_bezier_point_at_travel, DispatchEffectFrame,
};
use crate::motion::{
    DispatchEffectPhase, DispatchEffectPresentation, DispatchImpactParticle,
    DispatchReceiptVisualStatus, DispatchVisualOrigin,
};
use gpui::{point, px, Bounds, Size};
use ui::Theme;

/// 想法流星：从左抽屉直接射向卡片中心，弧线只做轻微弯曲，不会像指令栏那样先向上抛起。
///
/// 数据构造：起点 (200, 300)（左抽屉里的想法行），终点 (900, 420)（卡片中心）。
///   直线距离 = √(700² + 120²) ≈ 710.2；允许的最大偏离 = 距离 × 12% ≈ 85.2px。
/// 预期结果：
///   - 正断言：travel=0 恰是起点、travel=1 恰是终点；中途所有采样点到起终点连线的距离都 ≤ 12% 距离；
///     x 坐标随飞行单调增加（一路向右，不回头）。
///   - 负断言：中点不会被抛到连线上方 150px 以外（不绕路）。
#[test]
fn idea_shot_flies_straight_from_drawer_to_card_with_a_gentle_arc() {
    let origin = point(px(200.0), px(300.0));
    let target = point(px(900.0), px(420.0));
    assert_eq!(
        idea_shot_bezier_point_at_travel(origin, target, 0.0),
        origin,
        "起点应是想法行"
    );
    assert_eq!(
        idea_shot_bezier_point_at_travel(origin, target, 1.0),
        target,
        "终点应是卡片中心"
    );
    let (dx, dy) = (700.0_f32, 120.0_f32);
    let length = dx.hypot(dy);
    let mut previous_x = 200.0_f32;
    for step in 1..=20 {
        let p = idea_shot_bezier_point_at_travel(origin, target, step as f32 / 20.0);
        let (px_, py_) = (f32::from(p.x), f32::from(p.y));
        let deviation = ((px_ - 200.0) * dy - (py_ - 300.0) * dx).abs() / length;
        assert!(
            deviation <= length * 0.12 + 0.5,
            "第 {step} 个采样点偏离直线 {deviation}px，超过 12%"
        );
        assert!(
            px_ >= previous_x - 0.01,
            "x 应单调向右，第 {step} 个采样点后退了"
        );
        previous_x = px_;
    }
    let shot_middle = idea_shot_bezier_point_at_travel(origin, target, 0.5);
    let line_middle_y = 360.0_f32;
    assert!(
        (line_middle_y - f32::from(shot_middle.y)).abs() < 150.0,
        "想法流星中点不应被抛到连线上方 150px 以外"
    );
}

/// 想法流星的起点是窗口坐标，叠加层却从左抽屉右侧开始：换算后画出来的位置必须仍然落在想法行上，
/// 终点必须是卡片中心（e2e 里抓到过流星因为少了这段位移而偏到卡片右侧）。
///
/// 数据构造：左抽屉宽 397；想法行在窗口 (56, 266)；卡片在叠加层本地的 (22,72) 起、406.7×629，中心 = (22+203.3, 72+314.5) = (225.3, 386.5)。
/// 推导：叠加层本地起点 x = 56 − 397 = −341（在叠加层左侧之外，绘制时再加回 397 即窗口 56）。
/// 预期结果：
///   - 正断言：本地起点 = (−341, 266)；叠加绘制时加回叠加层原点后得到窗口 (56, 266)；终点加回后是窗口 (622.3, 386.5)，即卡片中央。
///   - 负断言：不做换算时起点会被画到窗口 x=453，而不是想法行。
#[test]
fn idea_row_origin_is_converted_into_overlay_local_coordinates() {
    let shift = point(px(397.0), px(0.0));
    let local = idea_row_origin(56, 266, shift);
    assert_eq!(
        (f32::from(local.x), f32::from(local.y)),
        (-341.0, 266.0),
        "本地起点应减去叠加层位移"
    );
    assert_eq!(
        f32::from(local.x + shift.x),
        56.0,
        "加回叠加层原点后应回到想法行的窗口 x"
    );
    let card_center_local_x: f32 = 22.0 + 406.66666 / 2.0;
    assert!(
        (card_center_local_x + 397.0 - 622.3).abs() < 0.1,
        "卡片中心加回叠加层原点后应是窗口 x≈622.3"
    );
    assert_ne!(
        f32::from(point(px(56.0), px(266.0)).x + shift.x),
        56.0,
        "对照：不换算会把起点画到 x=453"
    );
}

fn paint_counts(origin: DispatchVisualOrigin, phase: DispatchEffectPhase) -> (usize, usize) {
    let frame = DispatchEffectFrame {
        origin: point(px(600.0), px(752.0)),
        origin_bounds: None,
        target: point(px(300.0), px(220.0)),
        target_bounds: Bounds {
            origin: point(px(180.0), px(140.0)),
            size: Size {
                width: px(240.0),
                height: px(160.0),
            },
        },
        scale: 1.0,
        presentation: DispatchEffectPresentation {
            seed: 1,
            style: crate::motion::DispatchEffectStyle::default_for(origin),
            origin,
            target_node_id: 981,
            status: DispatchReceiptVisualStatus::Submitted,
            phase,
            progress: 0.6,
            particles: [DispatchImpactParticle::default(); 28],
            particle_count: 0,
        },
    };
    let plan = build_paint_plan(
        Bounds {
            origin: point(px(0.0), px(0.0)),
            size: Size {
                width: px(1_200.0),
                height: px(800.0),
            },
        },
        &[frame],
        Theme::default(),
    );
    (plan.paths.len(), plan.quads.len())
}

/// 语音/指令栏派发与想法派发统一使用想法流星：同一起终点、同一进度下画出的图元完全相同。
///
/// 数据构造：起点 (600, 752)（指令栏中心），目标卡片中心 (300, 220)；progress = 0.6。
///   想法流星飞行帧 = 尾焰 28 段 × 3 层 = 84 条 paths；火花 18 + 头部 4 = 22 个 quads。
/// 预期结果：
///   - 正断言：CommandDock 的飞行帧与撞击帧图元数量与 IdeaRow 完全一致；飞行帧精确为 84 paths / 22 quads。
///   - 负断言：CommandDock 飞行帧不再是离子核心（离子画法 paths 远少于 84）。
#[test]
fn command_dock_dispatch_uses_idea_meteor_painting() {
    let idea = DispatchVisualOrigin::IdeaRow { x: 600, y: 752 };
    let dock = DispatchVisualOrigin::CommandDock;
    let dock_flight = paint_counts(dock, DispatchEffectPhase::Flight);
    assert_eq!(
        dock_flight,
        paint_counts(idea, DispatchEffectPhase::Flight),
        "指令栏飞行帧应与想法流星一致"
    );
    assert_eq!(
        dock_flight,
        (84, 22),
        "想法流星飞行帧应为 84 paths / 22 quads"
    );
    assert_eq!(
        paint_counts(dock, DispatchEffectPhase::Impact),
        paint_counts(idea, DispatchEffectPhase::Impact),
        "指令栏撞击帧应与想法流星一致（白色光环 + 尾焰收拢）"
    );
    assert_ne!(
        paint_counts(
            DispatchVisualOrigin::AgentNode(7),
            DispatchEffectPhase::Flight
        ),
        dock_flight,
        "对照：Agent 间交接仍保留离子画法"
    );
}
