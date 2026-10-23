use super::{display_request, flag_enabled};

/// 场景：驱动传入虚拟显示器 id 时，隔离窗口放到该显示器上。
///
/// 数据构造：`NORMA_E2E_DISPLAY_ID` 为 "14"，以及带首尾空白的 " 14\n"；
/// 前台、可见开关都关闭。
///
/// 执行过程：分别调用 `display_request(false, false, Some(..))`。
///
/// 预期结果：
///   - 正断言：两者都解析为 Some(14)。
///   - 负断言：不会因空白退回 None（否则窗口会停靠在用户屏幕角落）。
#[test]
fn virtual_display_id_selects_target_display() {
    assert_eq!(
        display_request(false, false, Some("14")),
        Some(14),
        "数字 id 应选中虚拟显示器"
    );
    assert_eq!(
        display_request(false, false, Some(" 14\n")),
        Some(14),
        "首尾空白不应让 id 失效"
    );
}

/// 场景：前台或可见模式下，即使驱动传了 id，窗口也留在用户屏幕上。
///
/// 数据构造：id 为 "14"；分别打开 `NORMA_E2E_FOREGROUND` 与 `NORMA_E2E_VISIBLE`。
///
/// 执行过程：调用 `display_request(true, false, ..)` 与 `display_request(false, true, ..)`。
///
/// 预期结果：两者都返回 None，不把窗口移到虚拟显示器。
#[test]
fn foreground_or_visible_runs_ignore_virtual_display() {
    assert_eq!(
        display_request(true, false, Some("14")),
        None,
        "前台模式必须保持旧行为，不移窗口"
    );
    assert_eq!(
        display_request(false, true, Some("14")),
        None,
        "可见模式要让用户直接看到窗口"
    );
}

/// 场景：缺失或无效的 id 退回“停靠屏幕角落”，而不是选错显示器。
///
/// 数据构造：None、空串、"abc"、"-3"，以及 "0"（CGDirectDisplayID 0 表示创建失败）。
///
/// 执行过程：逐个调用 `display_request(false, false, ..)`。
///
/// 预期结果：全部返回 None。
#[test]
fn missing_or_invalid_display_id_falls_back_to_parking() {
    assert_eq!(display_request(false, false, None), None, "未传 id 应停靠");
    assert_eq!(display_request(false, false, Some("")), None, "空串不是 id");
    assert_eq!(
        display_request(false, false, Some("abc")),
        None,
        "非数字不是 id"
    );
    assert_eq!(
        display_request(false, false, Some("-3")),
        None,
        "负数不是 id"
    );
    assert_eq!(
        display_request(false, false, Some("0")),
        None,
        "0 表示驱动创建虚拟屏失败"
    );
}

/// 场景：E2E 开关只认精确的 "1"。
///
/// 数据构造：Some("1")、Some("0")、Some("true")、Some("")、None。
///
/// 执行过程：逐个调用 `flag_enabled`。
///
/// 预期结果：
///   - 正断言：只有 "1" 为 true。
///   - 负断言：其余都为 false，默认保持后台不抢焦点。
#[test]
fn switches_turn_on_only_for_exact_one() {
    assert!(flag_enabled(Some("1")), "\"1\" 应打开开关");
    assert!(!flag_enabled(Some("0")), "\"0\" 不应打开开关");
    assert!(!flag_enabled(Some("true")), "\"true\" 不应打开开关");
    assert!(!flag_enabled(Some("")), "空值不应打开开关");
    assert!(!flag_enabled(None), "未设置时保持默认后台模式");
}
