use super::*;

/// 场景：搜索260ms展开可在中途反向关闭，不从端点跳回。
/// 数据：起点0，目标1，t=65ms是四分之一时长。
/// 执行：采样起点/中途，再反向到0，推进260ms。
/// 预期：首帧仍0，中途严格在0和1之间，反向首帧连续，最后归0并停止。
#[test]
fn desktop_home_motion_is_interruptible() {
    let mut tween = HomeTween::default();
    let start = Instant::now();
    let duration = Duration::from_millis(260);
    tween.target(true, start, duration, false);
    assert_eq!(tween.value(start), 0.0, "展开从胶囊原位开始");
    let midway = start + Duration::from_millis(65);
    let value = tween.value(midway);
    assert!(value > 0.0 && value < 1.0, "中途尚未抵达终点");
    tween.target(false, midway, duration, false);
    assert_eq!(tween.value(midway), value, "反向不能跳变");
    assert_eq!(tween.value(midway + duration), 0.0, "完整收起");
    assert!(!tween.active(midway + duration), "完成后不请求动画帧");
}

/// 场景：减少动态效果立即落到目标状态。
/// 数据：常规时长200ms，reduce=true。
/// 执行：设置展开目标并在同一时刻采样。
/// 预期：立即为1且无活动动画；关闭也立即为0。
#[test]
fn desktop_home_motion_reduced_is_immediate() {
    let mut tween = HomeTween::default();
    let now = Instant::now();
    tween.target(true, now, Duration::from_millis(200), true);
    assert_eq!(tween.value(now), 1.0, "减少动态效果立即展开");
    assert!(!tween.active(now), "无后台动画帧");
    tween.target(false, now, Duration::from_millis(200), true);
    assert_eq!(tween.value(now), 0.0, "减少动态效果立即收起");
}
