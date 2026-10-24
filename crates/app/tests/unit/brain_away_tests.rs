use super::*;
use std::time::{Duration, Instant};

const TEN_MIN: Duration = Duration::from_secs(600);

/// 离开满阈值再回来给出离开时长；不满阈值、或没有离开过，都不触发。
///
/// 数据构造：离开 11 分钟后回来；离开 5 分钟后回来；一直在窗口里；回来后再次“回来”。
/// 预期结果：
///   - 正断言：11 分钟那次返回 Some(11 分钟)
///   - 负断言：5 分钟、没离开过、同一次离开的第二次回来，都是 None
#[test]
fn only_a_long_enough_absence_counts() {
    let t0 = Instant::now();
    let mut tracker = AwayTracker::default();
    assert_eq!(tracker.observe(true, t0, TEN_MIN), None, "一直在窗口里");
    assert_eq!(tracker.observe(false, t0, TEN_MIN), None, "离开本身不触发");
    let back = tracker.observe(true, t0 + Duration::from_secs(660), TEN_MIN);
    assert_eq!(back, Some(Duration::from_secs(660)));
    assert_eq!(
        tracker.observe(true, t0 + Duration::from_secs(700), TEN_MIN),
        None,
        "同一次离开只触发一次"
    );
    tracker.observe(false, t0 + Duration::from_secs(800), TEN_MIN);
    assert_eq!(
        tracker.observe(true, t0 + Duration::from_secs(1100), TEN_MIN),
        None,
        "只离开 5 分钟不触发"
    );
}

/// 离开期间反复失焦不重置起点：从第一次离开算起。
#[test]
fn repeated_deactivation_keeps_the_first_departure() {
    let t0 = Instant::now();
    let mut tracker = AwayTracker::default();
    tracker.observe(false, t0, TEN_MIN);
    tracker.observe(false, t0 + Duration::from_secs(300), TEN_MIN);
    assert_eq!(
        tracker.observe(true, t0 + Duration::from_secs(700), TEN_MIN),
        Some(Duration::from_secs(700))
    );
}

/// 阈值默认 10 分钟；环境变量只接受 1 到 86400 的整数。
#[test]
fn the_threshold_defaults_and_validates_the_override() {
    let secs = |v| parse_away_secs(v).as_secs();
    assert_eq!(secs(None), 600);
    assert_eq!(secs(Some("20")), 20);
    assert_eq!(secs(Some("0")), 600);
    assert_eq!(secs(Some("999999")), 600);
    assert_eq!(secs(Some("x")), 600);
}
