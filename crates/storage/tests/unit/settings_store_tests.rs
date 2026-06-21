use super::*;

/// S-1: SettingsStore 保存字符串配置后可覆盖并读回最新值。
///
/// 数据构造（含关键数值的推导过程）：
///   key = "theme"
///   first value = "dark"
///   second value = "light"，同 key 覆盖 first
///   missing key = "missing"，表中不存在，应返回 None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. set_string("theme", "dark") → 插入设置
///   2. set_string("theme", "light") → ON CONFLICT 覆盖设置
///   3. get_string("theme") 与 get_string("missing")
///
/// 预期结果：
///   - 正断言：theme 读回 Some("light")
///   - 正断言：missing 读回 None
///   - 负断言：theme 不应继续是旧值 "dark"
#[test]
fn test_settings_store_round_trips_and_overwrites_string_values() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SettingsStore::new(temp.path().join("app.sqlite"));

    store
        .set_string("theme", "dark")
        .expect("first setting write 应成功");
    store
        .set_string("theme", "light")
        .expect("second setting write 应覆盖成功");

    let theme = store.get_string("theme").expect("theme 应可读取");
    let missing = store.get_string("missing").expect("missing 应可读取");

    assert_eq!(
        theme.as_deref(),
        Some("light"),
        "theme 应读回最新覆盖值 light"
    );
    assert!(
        missing.is_none(),
        "不存在的 setting key 应返回 None，而不是构造默认值"
    );
    assert_ne!(
        theme.as_deref(),
        Some("dark"),
        "theme 不应继续保留旧值 dark，否则覆盖逻辑未生效"
    );
}
