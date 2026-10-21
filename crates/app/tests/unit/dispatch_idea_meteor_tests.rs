use super::*;

/// 想法流星的进度曲线：逐步加速、临近目标收速，两端精确落在 0 与 1。
///
/// 数据构造：把线性进度等分成 8 段（每段 12.5%），取每段内走过的行程。
/// 预期结果：
///   - 正断言：前半段每一段都比前一段走得更多（加速），后半段每一段都比前一段走得更少（收速）；曲线两端为 0 和 1。
///   - 负断言：第 1 段的行程不到最快一段的四分之一（起步确实慢，不是匀速）。
#[test]
fn meteor_progress_accelerates_then_settles() {
    assert_eq!(ease_in_out_cubic(0.0), 0.0, "起点应为 0");
    assert_eq!(ease_in_out_cubic(1.0), 1.0, "终点应为 1");
    let steps: Vec<f32> = (0..8)
        .map(|index| {
            ease_in_out_cubic((index + 1) as f32 / 8.0) - ease_in_out_cubic(index as f32 / 8.0)
        })
        .collect();
    for pair in steps[..4].windows(2) {
        assert!(pair[1] > pair[0], "前半段应逐步加速：{steps:?}");
    }
    for pair in steps[4..].windows(2) {
        assert!(pair[1] < pair[0], "后半段应逐步收速：{steps:?}");
    }
    let fastest = steps.iter().cloned().fold(0.0_f32, f32::max);
    assert!(steps[0] < fastest / 4.0, "起步应明显慢于最快段：{steps:?}");
}

/// 尾迹长度随速度变化：中段（最快）比起步和收尾都长。
///
/// 预期结果：
///   - 正断言：速度峰值在进度 0.5 附近，峰值是起点速度的数十倍。
///   - 负断言：起点与终点的速度都接近 0（不会一出发就拖一条长尾）。
#[test]
fn meteor_speed_peaks_in_the_middle() {
    let start = speed(0.01);
    let middle = speed(0.5);
    let end = speed(0.99);
    assert!(
        middle > start * 20.0 && middle > end * 20.0,
        "{start} {middle} {end}"
    );
    assert!(
        start < 0.01 && end < 0.01,
        "两端速度应接近 0：{start} {end}"
    );
}
