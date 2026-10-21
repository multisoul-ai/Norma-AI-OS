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

/// 尾焰随速度拉长：起步时很短，加速后最长，收尾时又缩短；任何时刻都不会超过整段行程。
///
/// 数据构造：取起步（15%）、中段（60%）、临近目标（98%）三个线性进度。
/// 预期结果：
///   - 正断言：中段的尾焰行程明显长于起步，也长于临近目标时。
///   - 负断言：尾焰行程不会为负，也不会超过 1。
#[test]
fn meteor_tail_stretches_with_speed() {
    let tail = |progress: f32| ease_in_out_cubic(progress) - tail_start(progress);
    let (early, middle, late) = (tail(0.15), tail(0.6), tail(0.98));
    assert!(
        middle > early * 3.0,
        "中段尾焰应远长于起步：{early} {middle}"
    );
    assert!(middle > late, "收尾时尾焰应缩短：{middle} {late}");
    for progress in [0.0, 0.15, 0.6, 0.98, 1.0] {
        let length = tail(progress);
        assert!((0.0..=1.0).contains(&length), "尾焰行程越界：{length}");
    }
}
