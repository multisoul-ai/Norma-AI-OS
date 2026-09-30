use super::load_bar_segment_left;

/// B-1: 进度线滑块必须从左侧完全移出开始、右侧完全移出结束，循环衔接处不可见。
///
/// 数据构造（含关键数值的推导过程）：
///   滑块宽度 SEGMENT = 0.32（占轨道宽度）；
///   progress = 0.0 → left = -0.32，滑块整体在轨道左侧之外；
///   progress = 1.0 → left = -0.32 + 1.0 * (1 + 0.32) = 1.0，滑块整体在轨道右侧之外；
///   progress = 0.5 → left = -0.32 + 0.5 * 1.32 = 0.34。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 取 progress 为 0、0.5、1 三个采样点
///   2. 另取越界输入 -1 与 2，验证被夹紧
///
/// 预期结果：
///   - 正断言：起点 -0.32、中点 0.34、终点 1.0
///   - 负断言：越界输入不得产生 -0.32 以下或 1.0 以上的位置，避免滑块飞出轨道后残留
#[test]
fn test_load_bar_segment_slides_from_outside_left_to_outside_right() {
    let eps = 1e-5;
    assert!(
        (load_bar_segment_left(0.0) - -0.32).abs() < eps,
        "起点必须完全在轨道左侧之外"
    );
    assert!(
        (load_bar_segment_left(0.5) - 0.34).abs() < eps,
        "中点位置应为 0.34"
    );
    assert!(
        (load_bar_segment_left(1.0) - 1.0).abs() < eps,
        "终点必须完全在轨道右侧之外"
    );
    assert!(
        (load_bar_segment_left(-1.0) - -0.32).abs() < eps,
        "progress 小于 0 必须夹紧到起点"
    );
    assert!(
        (load_bar_segment_left(2.0) - 1.0).abs() < eps,
        "progress 大于 1 必须夹紧到终点"
    );
}
