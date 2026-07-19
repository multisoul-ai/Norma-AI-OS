use super::{
    AgentProviderKind, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState,
    VoiceSettingsValues,
};

/// VS-10：Settings 状态变更只排队写操作，不得在 GPUI 调用路径同步写 SQLite。
///
/// 数据构造：临时 SettingsStore 初始无 active provider，加载 state 后选择 OpenAI。
/// 执行过程：选择后先直接查库，再 drain pending write 并在测试线程执行，最后再次查库。
/// 预期结果：执行 pending write 前数据库仍为空；执行后为 openai；队列只包含一次写入。
#[test]
fn test_agent_provider_persistence_is_deferred_until_background_write_executes() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());

    state.select_agent_provider(AgentProviderKind::OpenAiCompatible);
    let before = store
        .get_string(super::provider_values::KEY_AGENT_PROVIDER)
        .expect("后台写入前查询应成功");
    let writes = state.take_pending_writes();

    assert!(before.is_none(), "状态更新不得在 GPUI 路径同步写 SQLite");
    assert_eq!(writes.len(), 1, "一次 Provider 切换应只排队一次持久化");
    for write in writes {
        write.execute().expect("模拟后台写入应成功");
    }
    let after = store
        .get_string(super::provider_values::KEY_AGENT_PROVIDER)
        .expect("后台写入后查询应成功");
    assert_eq!(
        after.as_deref(),
        Some("openai"),
        "后台执行后应持久化 OpenAI"
    );
}

/// VS-11：已有旧版 Ollama 配置的用户升级后应保留 Provider，不被新默认值改写
///
/// 数据构造（含关键数值的推导过程）：
///   canonical active provider = 0（旧版本尚未保存此键）
///   legacy Ollama endpoint = 1
///   legacy dialogue model = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造仅包含旧版 endpoint/model 的 SavedVoiceSettings
///   2. 解析新双 Provider 配置结构
///   3. 读取 active provider 与保留的模型
///
/// 预期结果：
///   - 正断言：升级用户继续使用 Ollama 和原模型
///   - 负断言：OpenAI-compatible 只适用于真正的全新配置，不能覆盖旧用户
#[test]
fn legacy_ollama_configuration_preserves_provider_on_upgrade() {
    let saved = SavedVoiceSettings {
        ollama_endpoint: Some("http://legacy-ollama.test:11434".to_string()),
        dialogue_model: Some("legacy-qwen".to_string()),
        ..SavedVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());

    assert_eq!(
        values.agent_provider(),
        AgentProviderKind::Ollama,
        "legacy Ollama settings must preserve Ollama as the active provider"
    );
    assert_eq!(
        values.active_provider_profile().model(),
        "legacy-qwen",
        "legacy dialogue model must migrate into the active Ollama profile"
    );
    assert_ne!(
        values.agent_provider(),
        AgentProviderKind::OpenAiCompatible,
        "new OpenAI-compatible default must not switch an existing Ollama user"
    );
}
