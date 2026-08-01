use super::*;

/// PS-1：闲时动画降频配置在无记录和异常记录下均默认关闭，只接受明确布尔值。
///
/// 数据构造（含关键数值的推导过程）：
///   isolated stores = missing(1) + enabled(1) + disabled(1) + invalid(1) = 4 states
///   default throttle = false
///   accepted values = "true" / "false"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从空 SettingsStore 加载配置，读取默认值。
///   2. 写入 "true" 后重新加载，确认显式开启能恢复。
///   3. 写入 "false" 后重新加载，确认显式关闭能恢复。
///   4. 写入未知字符串后重新加载，确认安全回退到默认关闭。
///
/// 预期结果：
///   - 正断言："true" 必须恢复为开启，"false" 必须恢复为关闭。
///   - 负断言：空记录和异常记录不得意外开启闲时降频。
#[test]
fn performance_settings_load_idle_animation_throttling_with_off_default() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的 Performance Settings 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("performance.sqlite"));

    let missing = PerformanceSettingsState::load_from_store(store.clone());
    assert!(
        !missing.idle_animation_throttling_enabled(),
        "无持久化记录时闲时动画降频必须默认关闭"
    );

    store
        .set_string(IDLE_ANIMATION_THROTTLING_KEY, "true")
        .expect("应写入显式开启配置");
    let enabled = PerformanceSettingsState::load_from_store(store.clone());
    assert!(
        enabled.idle_animation_throttling_enabled(),
        "持久化值为 true 时必须恢复闲时动画降频"
    );

    store
        .set_string(IDLE_ANIMATION_THROTTLING_KEY, "false")
        .expect("应写入显式关闭配置");
    let disabled = PerformanceSettingsState::load_from_store(store.clone());
    assert!(
        !disabled.idle_animation_throttling_enabled(),
        "持久化值为 false 时必须保持闲时动画降频关闭"
    );

    store
        .set_string(IDLE_ANIMATION_THROTTLING_KEY, "unexpected")
        .expect("应写入异常配置以验证回退");
    let invalid = PerformanceSettingsState::load_from_store(store);
    assert!(
        !invalid.idle_animation_throttling_enabled(),
        "未知持久化值不得意外开启闲时动画降频"
    );
}
