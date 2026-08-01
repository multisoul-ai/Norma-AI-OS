use super::dispatch_receipt::{
    bezier_point, bezier_point_at_travel, build_paint_plan, command_dock_center, helix_offset,
    impact_radius_scale, natural_flight_progress, screen_point_visible, DispatchEffectFrame,
};
use crate::motion::{
    DispatchEffectPhase, DispatchEffectPresentation, DispatchImpactParticle,
    DispatchImpactParticleKind, DispatchReceiptVisualStatus,
};
use gpui::{point, px, Bounds, Size};
use ui::Theme;

/// DR-G-1：特效起点必须锚定真实 Command Dock 中央，不能复制漂移的近似坐标。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1200 × 800px
///   dock bottom = 22px；dock height = 52px
///   expected x = 1200 / 2 = 600px
///   expected y = 800 - 22 - 52 / 2 = 752px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用与真实 Dock 渲染共享的 geometry helper 输入 viewport。
///   2. 读取计算出的窗口坐标。
///
/// 预期结果：
///   - 正断言：起点为 (600, 752)，即 Dock 可见中心。
///   - 负断言：起点不能退化为窗口底边 (600, 800)。
#[test]
fn test_dispatch_effect_origin_matches_command_dock_center() {
    let center = command_dock_center(Size {
        width: px(1_200.0),
        height: px(800.0),
    });

    assert_eq!(
        center,
        point(px(600.0), px(752.0)),
        "Dock center 应为 x=1200/2=600、y=800-22-52/2=752"
    );
    assert_ne!(
        center,
        point(px(600.0), px(800.0)),
        "特效起点不得错误锚定到窗口底边"
    );
}

/// DR-G-2：三次贝塞尔轨迹必须精确连接 Dock 起点与动态 Agent 终点。
///
/// 数据构造（含关键数值的推导过程）：
///   origin = (600, 752)
///   target = (300, 220)
///   progress samples = 0、0.5、1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别计算 t=0、t=0.5、t=1 的轨迹坐标。
///   2. 对照传入的 source/target，并验证中点不是任一端点。
///
/// 预期结果：
///   - 正断言：t=0 精确等于 Dock origin，t=1 精确等于 Agent target。
///   - 正断言：t=0.5 位于真实曲线中间位置。
///   - 负断言：中点不能提前跳到 target 或停留在 origin。
#[test]
fn test_dispatch_bezier_connects_exact_source_and_target() {
    let origin = point(px(600.0), px(752.0));
    let target = point(px(300.0), px(220.0));
    let start = bezier_point(origin, target, 0.0);
    let midway = bezier_point(origin, target, 0.5);
    let end = bezier_point(origin, target, 1.0);

    assert_eq!(start, origin, "Bezier t=0 必须精确从 Command Dock 中央起飞");
    assert_eq!(end, target, "Bezier t=1 必须精确命中 Agent Card 当前中央");
    assert_ne!(midway, origin, "Bezier 中点不得停留在 Dock origin");
    assert_ne!(midway, target, "Bezier 中点不得提前跳到 Agent target");
    assert!(
        midway.x < origin.x && midway.x > target.x,
        "Bezier 中点 x 应位于 origin 与 target 之间；实际：{midway:?}"
    );
}

/// DR-G-3：目标卡片中心离开 viewport 时不得绘制跨屏飞行或离屏爆炸。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1200 × 800px
///   partially visible target center = (10, 160)，仍在 viewport 内
///   edge-only target center = (-10, 160)，中心 x=-10 < 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对卡片仍部分可见且中心在 viewport 内的 target 做判断。
///   2. 对只露出边缘但中心已在 viewport 外的 target 做判断。
///
/// 预期结果：
///   - 正断言：中心为 (10,160) 的 target 可以接收命中特效。
///   - 负断言：中心为 (-10,160) 的 target 不得创建 overlay frame。
#[test]
fn test_dispatch_effect_requires_target_center_inside_viewport() {
    let viewport = Size {
        width: px(1_200.0),
        height: px(800.0),
    };
    let center_visible = point(px(10.0), px(160.0));
    let center_outside = point(px(-10.0), px(160.0));

    assert!(
        screen_point_visible(center_visible, viewport),
        "target 中心 (10,160) 仍在 viewport 内，必须保留命中特效"
    );
    assert!(
        !screen_point_visible(center_outside, viewport),
        "target 仅露边缘但中心 x=-10 已离屏时不得绘制跨屏飞行"
    );
    assert_ne!(
        screen_point_visible(center_outside, viewport),
        true,
        "中心离屏 target 不得返回可播放状态"
    );
}

/// DR-G-4：Queued 命中必须同时降低亮度和爆发半径，不能只减少粒子数量。
///
/// 数据构造（含关键数值的推导过程）：
///   Submitted radius scale = 1.00
///   Queued radius scale = 0.72
///   radius reduction = 1.00 - 0.72 = 0.28（28%）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Submitted 与 Queued 的统一强度/半径比例。
///   2. 比较 Queued 比例与标准 Submitted 比例。
///
/// 预期结果：
///   - 正断言：Submitted 保持 1.0 标准半径，Queued 使用 0.72。
///   - 负断言：Queued 不得与 Submitted 使用相同爆发半径。
#[test]
fn test_queued_dispatch_impact_uses_smaller_radius_than_submitted() {
    let submitted = impact_radius_scale(DispatchReceiptVisualStatus::Submitted);
    let queued = impact_radius_scale(DispatchReceiptVisualStatus::Queued);

    assert_eq!(
        submitted, 1.0,
        "Submitted dispatch impact 必须保留标准 1.0 爆发半径"
    );
    assert_eq!(
        queued, 0.72,
        "Queued dispatch impact 必须使用 0.72 半径，表达已排队而非已提交"
    );
    assert_ne!(
        queued, submitted,
        "Queued 不得只降低 alpha/粒子数量而仍沿用 Submitted 的完整半径"
    );
    assert!(
        queued < submitted,
        "Queued 爆发半径必须小于 Submitted；submitted={submitted}, queued={queued}"
    );
}

/// DR-G-5：离子飞行必须按曲线弧长推进，并用轻微前冲节奏避免参数曲线忽快忽慢。
///
/// 数据构造（含关键数值的推导过程）：
///   origin = (600, 752)，target = (300, 220)
///   travel samples = 0、0.25、0.50、0.75、1.00
///   natural midpoint = 0.50 + sin(π × 0.50) × 0.09 = 0.59
///   acceptable sampled-distance ratio = longest / shortest ≤ 1.30
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 验证自然节奏映射保持首尾精确，并将 50% 时间映射到 59% 路程。
///   2. 按四个等弧长区间采样曲线上的五个坐标。
///   3. 比较四段直线近似距离，确认视觉速度没有明显突变。
///
/// 预期结果：
///   - 正断言：自然节奏严格保持 0 与 1，且中点为 0.59。
///   - 正断言：弧长采样的最长/最短距离比例不超过 1.30。
///   - 负断言：中点不得退化为线性 0.50，终点不得偏离 Agent 中央。
#[test]
fn test_dispatch_flight_uses_arc_length_and_natural_travel() {
    let origin = point(px(600.0), px(752.0));
    let target = point(px(300.0), px(220.0));

    assert_eq!(
        natural_flight_progress(0.0),
        0.0,
        "自然飞行节奏必须从精确的 0 路程开始"
    );
    assert!(
        (natural_flight_progress(0.5) - 0.59).abs() < 0.000_001,
        "50% 时间应映射为 0.50+sin(π×0.50)×0.09=0.59 路程"
    );
    assert_ne!(
        natural_flight_progress(0.5),
        0.5,
        "离子飞行不得退化为机械的线性时间映射"
    );
    assert_eq!(
        natural_flight_progress(1.0),
        1.0,
        "自然飞行节奏必须精确收束到目标"
    );

    let p0 = bezier_point_at_travel(origin, target, 0.0);
    let p1 = bezier_point_at_travel(origin, target, 0.25);
    let p2 = bezier_point_at_travel(origin, target, 0.50);
    let p3 = bezier_point_at_travel(origin, target, 0.75);
    let p4 = bezier_point_at_travel(origin, target, 1.0);
    let d1 = point_distance(p0, p1);
    let d2 = point_distance(p1, p2);
    let d3 = point_distance(p2, p3);
    let d4 = point_distance(p3, p4);
    let shortest = d1.min(d2).min(d3).min(d4);
    let longest = d1.max(d2).max(d3).max(d4);

    assert_eq!(p0, origin, "弧长采样起点必须精确等于 Dock 中央");
    assert_eq!(p4, target, "弧长采样终点必须精确等于 Agent 卡片中央");
    assert!(
        longest / shortest <= 1.30,
        "四段等路程采样的近似距离比例应≤1.30；实际为 {}",
        longest / shortest
    );
    assert_ne!(p2, origin, "50% 弧长采样不得仍停留在 Dock");
    assert_ne!(p2, target, "50% 弧长采样不得提前到达 Agent 中央");
}

/// DR-G-6：双螺旋尾迹必须在核心两侧对称盘旋，并在尾迹端点自然收束。
///
/// 数据构造（含关键数值的推导过程）：
///   helix ratio samples = 0、0.50、1.00
///   phase A = 0；phase B = π
///   envelope = sin(π × ratio)，所以两端 envelope = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别计算两条相差 π 的螺旋在首端、中点、尾端的法线偏移。
///   2. 验证端点收束，中点非零且两股偏移互为相反数。
///
/// 预期结果：
///   - 正断言：两股螺旋在 ratio=0 与 1 时都归零。
///   - 正断言：ratio=0.50 时两股偏移非零且对称。
///   - 负断言：双螺旋中点不得重合成一条无结构直线。
#[test]
fn test_dispatch_flight_helix_is_symmetric_and_tapered() {
    let phase_a = 0.0;
    let phase_b = std::f32::consts::PI;
    let a_start = helix_offset(0.0, phase_a);
    let a_mid = helix_offset(0.5, phase_a);
    let a_end = helix_offset(1.0, phase_a);
    let b_start = helix_offset(0.0, phase_b);
    let b_mid = helix_offset(0.5, phase_b);
    let b_end = helix_offset(1.0, phase_b);

    assert_eq!(a_start, 0.0, "第一股螺旋必须在尾迹起端收束到主轨迹");
    assert_eq!(a_end, 0.0, "第一股螺旋必须在离子核心端收束到主轨迹");
    assert!(
        b_start.abs() < 0.000_001,
        "第二股螺旋必须在尾迹起端收束到主轨迹"
    );
    assert!(
        b_end.abs() < 0.000_001,
        "第二股螺旋必须在离子核心端收束到主轨迹"
    );
    assert!(a_mid.abs() > 0.50, "第一股螺旋中点必须明显离开主轨迹");
    assert!(b_mid.abs() > 0.50, "第二股螺旋中点必须明显离开主轨迹");
    assert!(
        (a_mid + b_mid).abs() < 0.000_001,
        "相差 π 的双螺旋中点必须互为相反数；a={a_mid}, b={b_mid}"
    );
    assert_ne!(a_mid, b_mid, "双螺旋中点不得重合成单条直线");
}

fn point_distance(a: gpui::Point<gpui::Pixels>, b: gpui::Point<gpui::Pixels>) -> f32 {
    let dx = f32::from(b.x - a.x);
    let dy = f32::from(b.y - a.y);
    dx.hypot(dy)
}

/// DR-G-7：参考页完整命中帧必须把同类线段批处理到单 effect 16 paths / 36 quads 的 GPUI 绘制预算内。
///
/// 数据构造（含关键数值的推导过程）：
///   impact progress = 0.28（星芒、flare、主粒子与延迟 Spark 同时可见）
///   particles = 16 Comet + 4 Mote + 8 Spark = 28
///   expected paths = shockwave(1) + star(1) + arcs(3) + flare(1) + Comet size groups(4) + Spark batch(1) = 11
///   expected quads = card outline(1) + central flash(4) + flare(1) + Comet heads(16) + Mote layers(8) = 30
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个 Submitted Impact presentation 和固定 28 粒子池。
///   2. 在最密集的 28% 生命周期位置生成 GPUI paint plan。
///   3. 读取实际 path/quad 数量并对照预分配上限。
///
/// 预期结果：
///   - 正断言：密集命中帧精确生成 11 paths 与 30 quads。
///   - 正断言：数量分别不超过 16/36 的单 effect 硬预算。
///   - 负断言：paint plan 不得为空，也不得退化为只有旧版圆点 quads。
#[test]
fn test_dispatch_ion_impact_stays_within_gpui_primitive_budget() {
    let mut particles = [DispatchImpactParticle::default(); 28];
    for (index, particle) in particles.iter_mut().enumerate() {
        particle.kind = if index >= 20 {
            DispatchImpactParticleKind::Spark
        } else if index % 5 == 0 {
            DispatchImpactParticleKind::Mote
        } else {
            DispatchImpactParticleKind::Comet
        };
        particle.direction_x = (std::f32::consts::TAU * index as f32 / 28.0).cos();
        particle.direction_y = (std::f32::consts::TAU * index as f32 / 28.0).sin();
        particle.distance = if index >= 20 { 68.0 } else { 108.0 };
        particle.size = if index >= 20 {
            1.6
        } else {
            2.1 + (index % 4) as f32 * 0.45
        };
        particle.delay = if index >= 20 { 0.15 } else { 0.0 };
        particle.lifetime = 1.0;
    }
    let frame = DispatchEffectFrame {
        origin: point(px(600.0), px(752.0)),
        target: point(px(300.0), px(220.0)),
        target_bounds: Bounds {
            origin: point(px(180.0), px(140.0)),
            size: Size {
                width: px(240.0),
                height: px(160.0),
            },
        },
        presentation: DispatchEffectPresentation {
            target_node_id: 981,
            status: DispatchReceiptVisualStatus::Submitted,
            phase: DispatchEffectPhase::Impact,
            progress: 0.28,
            reduce_motion: false,
            particles,
            particle_count: 28,
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

    assert_eq!(
        plan.paths.len(),
        11,
        "最密集命中帧应批处理为 1+1+3+1+4+1=11 条 paths"
    );
    assert_eq!(
        plan.quads.len(),
        30,
        "最密集命中帧应精确生成 1+4+1+16+8=30 个 quads"
    );
    assert!(
        plan.paths.len() <= 16,
        "单 effect path 数不得超过批处理后的硬预算 16；实际为 {}",
        plan.paths.len()
    );
    assert!(
        plan.quads.len() <= 36,
        "单 effect quad 数不得超过预分配硬预算 36；实际为 {}",
        plan.quads.len()
    );
    assert_ne!(plan.paths.len(), 0, "离子命中不得退化为空 path paint plan");
    assert_ne!(
        plan.quads.len(),
        18,
        "离子命中不得回退为旧版 18 个单层圆点 quad"
    );
}
