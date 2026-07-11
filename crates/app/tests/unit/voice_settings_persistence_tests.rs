use super::*;

/// VS-11：Settings 持久化必须单飞串行，后续修改不能与首批写入并行。
///
/// 数据构造：先选择 OpenAI 并取走首批写入，再在首批未完成时切回 Ollama。
/// 执行过程：第二次尝试 drain 应为空；记录首批完成后再次 drain 并执行。
/// 预期结果：两批各一个写入且严格串行；数据库最终值为最后选择的 Ollama。
#[test]
fn test_voice_settings_persistence_serializes_consecutive_writes() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());

    state.select_agent_provider(AgentProviderKind::OpenAiCompatible);
    let first = state.take_pending_writes();
    state.select_agent_provider(AgentProviderKind::Ollama);
    let while_running = state.take_pending_writes();

    assert_eq!(first.len(), 1, "首批应只包含 OpenAI 选择写入");
    assert!(
        while_running.is_empty(),
        "首批未完成时不得启动第二个并行持久化任务"
    );
    for write in first {
        write.execute().expect("首批后台写入应成功");
    }
    state.record_persistence_result(Ok(()));
    let second = state.take_pending_writes();
    assert_eq!(second.len(), 1, "首批完成后应释放后续 Ollama 写入");
    for write in second {
        write.execute().expect("第二批后台写入应成功");
    }
    state.record_persistence_result(Ok(()));

    assert_eq!(
        store
            .get_string(super::provider_values::KEY_AGENT_PROVIDER)
            .expect("最终 Provider 查询应成功")
            .as_deref(),
        Some("ollama"),
        "串行执行后数据库必须保留最后一次选择"
    );
}

/// 同一 Voice 字段的连续自动保存只保留最新待写值，并可从 SQLite 重新加载。
///
/// 场景：用户快速把 Model 依次输入为 `a`、`ab`、`abc`，后台任务尚未开始取写队列。
/// 数据构造：三个值使用严格前缀关系，可区分逐次写入与 latest-wins 合并；临时
/// SettingsStore 隔离真实用户配置。
/// 执行过程：进入 Model 编辑态后连续调用自动保存入口，取出待写队列、执行并记录成功，
/// 再从同一个 store 创建新的 VoiceSettingsState。
/// 预期结果：待写队列只有一个写入；编辑态在自动保存时保持；重新加载值精确为 `abc`，
/// 不得残留较早的 `a` 或 `ab`。
#[test]
fn test_voice_settings_auto_save_coalesces_latest_value_and_reloads() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.begin_edit(VoiceSettingsField::AgentModel);

    state.auto_save_edit_value(VoiceSettingsField::AgentModel, "a".to_string());
    state.auto_save_edit_value(VoiceSettingsField::AgentModel, "ab".to_string());
    state.auto_save_edit_value(VoiceSettingsField::AgentModel, "abc".to_string());

    assert_eq!(
        state.editing_field(),
        Some(VoiceSettingsField::AgentModel),
        "自动保存期间必须保持 Model 编辑态"
    );
    assert_eq!(
        state.active_agent_model(),
        "abc",
        "前台正式 Model 应立即采用最新稳定输入"
    );
    let writes = state.take_pending_writes();
    assert_eq!(writes.len(), 1, "同一 key 的连续自动保存应合并为一次待写");
    for write in writes {
        write.execute().expect("自动保存的后台写入应成功");
    }
    state.record_persistence_result(Ok(()));

    let reloaded = VoiceSettingsState::load_from_store(store);
    assert_eq!(
        reloaded.active_agent_model(),
        "abc",
        "重新加载后应读到 latest-wins 的最终 Model"
    );
    assert_ne!(
        reloaded.active_agent_model(),
        "a",
        "重新加载后不得落后到第一次自动保存值"
    );
    assert_ne!(
        reloaded.active_agent_model(),
        "ab",
        "重新加载后不得落后到第二次自动保存值"
    );
}

/// 必填 Provider 字段清空时不得覆盖已保存值或产生删除写入。
///
/// 场景：用户全选 Model 后按删除，使输入框暂时为空，再准备继续输入或按 Escape。
/// 数据构造：先记录默认有效 Model，再以空字符串调用自动保存；临时 SettingsStore 用于
/// 观察是否错误地产生 SQLite 删除任务。
/// 执行过程：进入 Model 编辑态，尝试自动保存空值，随后读取返回值、正式设置和待写队列。
/// 预期结果：自动保存返回 false，正式 Model 保持原值且队列为空；编辑态仍保留，允许用户
/// 继续输入，不能把暂时空白当作有效配置写入数据库。
#[test]
fn test_voice_settings_auto_save_rejects_empty_required_value() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    let original = state.active_agent_model().to_string();
    state.begin_edit(VoiceSettingsField::AgentModel);

    let saved = state.auto_save_edit_value(VoiceSettingsField::AgentModel, String::new());

    assert_eq!(saved, None, "空 Model 不应被自动保存为有效设置");
    assert_eq!(
        state.active_agent_model(),
        original,
        "空 Model 应保留输入前的正式有效值"
    );
    assert_eq!(
        state.editing_field(),
        Some(VoiceSettingsField::AgentModel),
        "拒绝空值后应继续保持 Model 编辑态"
    );
    assert!(
        state.take_pending_writes().is_empty(),
        "拒绝空 Model 时不得产生 SQLite Set 或 Delete 写入"
    );
}

/// API Key 的空白自动保存不得代替明确的 Clear 操作删除已保存密钥。
///
/// 场景：用户已有 API Key，聚焦安全空白输入框后产生一次空内容自动保存；API Key
/// 输入框不会回填明文，因此空内容本身不能表达删除意图。
/// 数据构造：先把测试密钥写入临时 SettingsStore 并完成持久化，再从同一状态进入
/// AgentApiKey 编辑态；断言只检查 configured 状态，不输出密钥内容。
/// 执行过程：以空字符串调用自动保存入口，检查返回值、正式 Provider 配置、待写队列，
/// 最后从同一个 SQLite 重新加载状态，再调用明确的 Clear 并执行其待写任务。
/// 预期结果：空值被拒绝为隐式删除，当前与重载状态均保持 configured，且不得产生
/// SQLite Delete；明确 Clear 后则产生一次 Delete，重载状态变为未配置。
#[test]
fn test_voice_settings_auto_save_empty_api_key_preserves_saved_secret() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.begin_edit(VoiceSettingsField::AgentApiKey);
    let configured =
        state.auto_save_edit_value(VoiceSettingsField::AgentApiKey, "test-api-key".to_string());
    assert!(configured.is_some(), "测试前置 API Key 应被状态层接受");
    let setup_writes = state.take_pending_writes();
    assert_eq!(setup_writes.len(), 1, "测试前置应产生一个 API Key 写入");
    for write in setup_writes {
        write.execute().expect("测试前置 API Key 写入应成功");
    }
    state.record_persistence_result(Ok(()));

    state.begin_edit(VoiceSettingsField::AgentApiKey);
    let saved = state.auto_save_edit_value(VoiceSettingsField::AgentApiKey, String::new());

    assert_eq!(saved, None, "空 API Key 不应被自动保存为删除操作");
    assert!(
        state.values().active_provider_profile().api_key().is_some(),
        "空白自动保存后当前 Provider 应继续保持 API Key configured"
    );
    assert!(
        state.take_pending_writes().is_empty(),
        "空白 API Key 自动保存不得产生 SQLite Delete 写入"
    );
    let reloaded = VoiceSettingsState::load_from_store(store.clone());
    assert!(
        reloaded
            .values()
            .active_provider_profile()
            .api_key()
            .is_some(),
        "从 SQLite 重新加载后 API Key 仍应保持 configured"
    );

    state.clear_agent_api_key();
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "明确 Clear 后当前 Provider 应立即变为未配置"
    );
    let clear_writes = state.take_pending_writes();
    assert_eq!(clear_writes.len(), 1, "明确 Clear 应产生一次 SQLite Delete");
    for write in clear_writes {
        write.execute().expect("明确 Clear 的 Delete 写入应成功");
    }
    state.record_persistence_result(Ok(()));
    let cleared = VoiceSettingsState::load_from_store(store);
    assert!(
        cleared
            .values()
            .active_provider_profile()
            .api_key()
            .is_none(),
        "明确 Clear 后从 SQLite 重载应保持未配置"
    );
}

/// 编辑 API Key 时执行 Clear，必须同时结束编辑并清空状态层草稿。
///
/// 场景：用户正在输入替换密钥时点击 Clear；输入实体会同步为空，状态层也不能继续保留
/// `editing=AgentApiKey` 或密钥草稿。
/// 数据构造：使用 in-memory state 与测试草稿；断言只检查空值，不打印草稿内容。
/// 执行过程：进入 API Key 编辑态、设置草稿、调用 clear，再读取编辑字段与草稿状态。
/// 预期结果：编辑态结束且草稿为空；当前 Provider 为未配置。
#[test]
fn test_voice_settings_clear_api_key_cancels_edit_and_draft() {
    let mut state = VoiceSettingsState::in_memory_default();
    state.begin_edit_with_value(
        VoiceSettingsField::AgentApiKey,
        "replacement-test-key".to_string(),
    );

    state.clear_agent_api_key();

    assert_eq!(
        state.editing_field(),
        None,
        "Clear 后 API Key 编辑态必须立即结束"
    );
    assert!(
        state.draft.is_empty(),
        "Clear 后状态层不得继续持有 API Key 草稿"
    );
    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "Clear 后当前 Provider 应为未配置"
    );
}

/// Clear 必须同时移除 canonical 与旧版 OpenAI API Key。
///
/// 场景：旧版本只在 `voice.openai.api_key` 保存密钥，升级后用户点击 Clear；重启后旧 key
/// 不得通过 fallback 再次出现。
/// 数据构造：临时 SQLite 只写 active=openai 与 legacy API Key，不写 canonical key。
/// 执行过程：加载状态、调用 clear、执行全部待写任务，再从同一个 SQLite 重载。
/// 预期结果：canonical 与 legacy 两个存储键都不存在；重载后 API Key 为未配置。
#[test]
fn test_voice_settings_clear_api_key_removes_legacy_fallback() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string(super::provider_values::KEY_AGENT_PROVIDER, "openai")
        .expect("测试前置应保存 OpenAI provider");
    store
        .set_string(super::values::KEY_OPENAI_API_KEY, "legacy-test-key")
        .expect("测试前置应保存 legacy API Key");
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    assert!(
        state.values().active_provider_profile().api_key().is_some(),
        "测试前置 legacy API Key 应通过 fallback 加载为 configured"
    );
    state.clear_agent_api_key();

    assert!(
        state.values().active_provider_profile().api_key().is_none(),
        "Clear 后当前 OpenAI profile 应立即变为未配置"
    );
    let writes = state.take_pending_writes();
    assert!(!writes.is_empty(), "OpenAI Clear 应产生后台持久化写入");
    for write in writes {
        write.execute().expect("API Key Clear 写入应成功");
    }
    state.record_persistence_result(Ok(()));
    assert!(
        store
            .get_string(super::provider_values::KEY_AGENT_OPENAI_API_KEY)
            .expect("canonical API Key 查询应成功")
            .is_none(),
        "Clear 后 canonical OpenAI API Key 存储键必须不存在"
    );
    assert!(
        store
            .get_string(super::values::KEY_OPENAI_API_KEY)
            .expect("legacy API Key 查询应成功")
            .is_none(),
        "Clear 后 legacy OpenAI API Key 存储键必须不存在"
    );
    let reloaded = VoiceSettingsState::load_from_store(store);
    assert!(
        reloaded
            .values()
            .active_provider_profile()
            .api_key()
            .is_none(),
        "重载后 legacy fallback 不得令已 Clear 的 API Key 复活"
    );
}
