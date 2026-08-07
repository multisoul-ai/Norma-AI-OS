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

/// S-2: SettingsStore 删除字符串配置后读取应回到 None，供 Settings 的 Use default 使用。
///
/// 数据构造（含关键数值的推导过程）：
///   key = "voice.paraformer.model_dir"
///   value = "/tmp/model"，模拟用户选择了自定义模型目录
///   delete count = 1，删除后该 key 不应继续参与配置解析
///
/// 执行过程（逐步说明系统如何处理）：
///   1. set_string(key, value) → 插入 voice 模型目录配置
///   2. delete_string(key) → 删除该配置
///   3. get_string(key) → 查询删除后的值
///
/// 预期结果：
///   - 正断言：删除前能读回 "/tmp/model"
///   - 正断言：删除后读回 None，Settings 可恢复默认解析
///   - 负断言：删除后不应继续返回旧路径
#[test]
fn test_settings_store_deletes_string_values() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SettingsStore::new(temp.path().join("app.sqlite"));
    let key = "voice.paraformer.model_dir";

    store
        .set_string(key, "/tmp/model")
        .expect("voice model dir setting write 应成功");
    let before_delete = store.get_string(key).expect("setting 应可读取");

    store
        .delete_string(key)
        .expect("delete_string 应删除已有 setting");
    let after_delete = store.get_string(key).expect("deleted setting 应可查询");

    assert_eq!(
        before_delete.as_deref(),
        Some("/tmp/model"),
        "删除前应能读回自定义 voice 模型目录"
    );
    assert!(
        after_delete.is_none(),
        "delete_string 后 setting 应恢复为 None，供 Use default 使用"
    );
    assert_ne!(
        after_delete.as_deref(),
        Some("/tmp/model"),
        "删除后不应继续返回旧 voice 模型目录"
    );
}

/// S-3: 一组关联设置必须在同一个 SQLite 事务中同时写入和删除。
///
/// 数据构造（含关键数值的推导过程）：
///   initial keys = 2（provider=local，legacy_voice=old）
///   mutations = 3（provider=minimax，model=speech-2.8-turbo，删除 legacy_voice）
///   final keys = 2（provider + model；legacy_voice 被删除）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入旧 provider 与旧 voice，构造切换前状态。
///   2. 调用 apply_string_changes，一次提交完整的新 TTS tuple。
///   3. 分别读取三个 key，验证事务后的最终快照。
///
/// 预期结果：
///   - 正断言：provider 与 model 同时呈现新值。
///   - 正断言：要求删除的 legacy_voice 返回 None。
///   - 负断言：provider 不得继续保留 local，legacy_voice 不得残留 old。
#[test]
fn test_settings_store_applies_related_string_changes_atomically() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("tts.provider", "local")
        .expect("旧 TTS provider 应写入成功");
    store
        .set_string("tts.legacy_voice", "old")
        .expect("旧 TTS voice 应写入成功");

    store
        .apply_string_changes(&[
            ("tts.provider".to_string(), Some("minimax".to_string())),
            (
                "tts.model".to_string(),
                Some("speech-2.8-turbo".to_string()),
            ),
            ("tts.legacy_voice".to_string(), None),
        ])
        .expect("关联 TTS 设置应在一个事务中提交成功");

    let provider = store
        .get_string("tts.provider")
        .expect("事务后的 provider 应可读取");
    let model = store
        .get_string("tts.model")
        .expect("事务后的 model 应可读取");
    let legacy_voice = store
        .get_string("tts.legacy_voice")
        .expect("事务后的 legacy voice 应可查询");
    assert_eq!(
        provider.as_deref(),
        Some("minimax"),
        "事务提交后 provider 必须切换为 minimax"
    );
    assert_eq!(
        model.as_deref(),
        Some("speech-2.8-turbo"),
        "事务提交后 model 必须与 provider 同批写入"
    );
    assert!(legacy_voice.is_none(), "事务提交后 legacy_voice 必须被删除");
    assert_ne!(
        provider.as_deref(),
        Some("local"),
        "事务提交后 provider 不得继续保留旧值 local"
    );
    assert_ne!(
        legacy_voice.as_deref(),
        Some("old"),
        "事务提交后不得残留旧 voice"
    );
}
