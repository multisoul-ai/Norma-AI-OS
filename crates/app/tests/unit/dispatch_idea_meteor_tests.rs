use super::*;

/// 想法流星的进度曲线：先缓慢蓄势，随后突然加速，撞击时最快；两端精确落在 0 与 1。
///
/// 数据构造：把线性进度等分成 8 段，取每段内走过的行程。
/// 预期结果：
///   - 正断言：每一段都比前一段走得更多（持续加速）；前半程总行程不到整段的 10%（蓄势）；最后四分之一占整段的一半以上（突然冲刺）。
///   - 负断言：曲线两端为 0 和 1，不会越界。
#[test]
fn meteor_charges_slowly_then_bursts() {
    assert_eq!(ease_in_out_cubic(0.0), 0.0, "起点应为 0");
    assert_eq!(ease_in_out_cubic(1.0), 1.0, "终点应为 1");
    let steps: Vec<f32> = (0..8)
        .map(|index| {
            ease_in_out_cubic((index + 1) as f32 / 8.0) - ease_in_out_cubic(index as f32 / 8.0)
        })
        .collect();
    for pair in steps.windows(2) {
        assert!(pair[1] > pair[0], "应持续加速：{steps:?}");
    }
    assert!(ease_in_out_cubic(0.5) < 0.10, "前半程应几乎在原地蓄势");
    assert!(
        1.0 - ease_in_out_cubic(0.75) > 0.5,
        "最后四分之一时间应走完一半以上的路程"
    );
}

/// 尾焰随速度拉长：蓄势期很短，冲刺时越拖越长；任何时刻都不会超过整段行程。
///
/// 数据构造：取蓄势（15%）、冲刺（60%）、临近目标（98%）三个线性进度。
/// 预期结果：
///   - 正断言：冲刺时的尾焰行程明显长于蓄势期；临近目标时仍不短于冲刺初期。
///   - 负断言：尾焰行程不会为负，也不会超过 1。
#[test]
fn meteor_tail_stretches_with_speed() {
    let tail = |progress: f32| ease_in_out_cubic(progress) - tail_start(progress);
    let (early, middle, late) = (tail(0.15), tail(0.6), tail(0.98));
    assert!(
        middle > early * 3.0,
        "中段尾焰应远长于起步：{early} {middle}"
    );
    assert!(
        late > middle,
        "越接近目标越快，尾焰不应缩短：{middle} {late}"
    );
    for progress in [0.0, 0.15, 0.6, 0.98, 1.0] {
        let length = tail(progress);
        assert!((0.0..=1.0).contains(&length), "尾焰行程越界：{length}");
    }
}
