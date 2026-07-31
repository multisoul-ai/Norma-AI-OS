use super::dispatch_receipt::{
    bezier_point, command_dock_center, impact_radius_scale, screen_point_visible,
};
use crate::motion::DispatchReceiptVisualStatus;
use gpui::{point, px, Size};

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
