use super::*;

/// GPU 时间戳必须来自完成后的有效区间，缺失数据不能当作零耗时通过。
///
/// 数据构造（含关键数值的推导过程）：
///   正常区间 10.000s → 10.004s，耗时 (10.004 - 10.000) × 1000 = 4ms。
///   Metal 尚未提供时间戳时返回 0；另构造逆序和非有限值。
/// 执行过程：
///   1. 转换正常区间为毫秒。
///   2. 分别输入缺失、逆序、NaN 和无穷时间戳。
/// 预期结果：
///   - 正常区间得到 4ms。
///   - 无效区间全部返回 None，不生成可用于性能通过的数值。
#[test]
fn gpu_timing_rejects_missing_and_invalid_timestamps() {
    let elapsed = elapsed_ms(10.0, 10.004).expect("有效 GPU 时间戳必须可转换");
    // 浮点减法误差容差 0.000001ms，远小于 16.67ms 帧预算。
    assert!((elapsed - 4.0).abs() < 0.000001, "有效 GPU 区间应为 4ms");
    assert_eq!(elapsed_ms(0.0, 0.0), None, "缺失时间戳不能当零耗时");
    assert_eq!(elapsed_ms(0.0, 10.0), None, "缺失起点不能进入统计");
    assert_eq!(elapsed_ms(10.0, 9.0), None, "逆序时间戳不能进入统计");
    assert_eq!(elapsed_ms(f64::NAN, 10.0), None, "NaN 起点不能进入统计");
    assert_eq!(
        elapsed_ms(10.0, f64::INFINITY),
        None,
        "无穷终点不能进入统计"
    );
}
