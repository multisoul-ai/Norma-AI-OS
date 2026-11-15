use super::task_time::{avatar_color, avatar_initial, task_time_label};

/// 场景：侧栏任务行右侧的时间。
/// 数据构造：2026-11-15 10:12 UTC（时区偏移 +8h，本地 18:12）；同一天更晚的现在；前一天；跨月的旧日期。
/// 预期结果：同一天显示本地「18:12」，不是 UTC 的 10:12；前一天和更早显示「月/日」。
#[test]
fn time_label_is_clock_today_and_date_otherwise() {
    let updated = 1_794_737_520; // 2026-11-15 10:12:00 UTC
    let eight = 8 * 3600;
    assert_eq!(
        task_time_label(updated, updated + 600, eight),
        "18:12",
        "check: same day"
    );
    assert_eq!(
        task_time_label(updated, updated + 86_400, eight),
        "11/15",
        "check: yesterday"
    );
    assert_eq!(
        task_time_label(updated, updated + 40 * 86_400, eight),
        "11/15",
        "check: older"
    );
    // UTC 23:30 在 +8 时区已经是第二天 07:30，和「现在」(+8 的第二天) 算同一天。
    let late = 1_794_786_600; // 2026-11-15 23:50:00 UTC
    assert_eq!(
        task_time_label(late, late + 60, eight),
        "7:50",
        "check: crosses local midnight"
    );
}

/// 场景：头像颜色和首字。
/// 数据构造：同名多次取色，不同名取色；空名字；中文名。
/// 预期结果：同名颜色稳定；首字大写；空名为问号；中文取第一个字。
#[test]
fn avatar_is_stable_per_name() {
    assert_eq!(
        avatar_color("Henry"),
        avatar_color("Henry"),
        "check: stable color"
    );
    assert_eq!(avatar_initial("henry"), "H", "check: uppercase");
    assert_eq!(avatar_initial(""), "?", "check: empty name");
    assert_eq!(avatar_initial("幕僚"), "幕", "check: cjk");
}
