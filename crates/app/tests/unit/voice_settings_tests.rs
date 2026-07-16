use super::provider_values::DEFAULT_OLLAMA_ENDPOINT;
use super::*;

/// VS-1: 已保存的 voice 设置应优先于环境变量，避免 Settings UI 被 env 覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   saved endpoint = http://saved:11434
///   env endpoint   = http://env:11434
///   saved planner  = saved-planner
///   env planner    = env-planner
///   saved refine   = saved-refine
///   env refine     = env-refine
///   saved trace    = false，env trace = "true"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 SavedVoiceSettings，模拟 SettingsStore 中已有值
///   2. 构造 EnvVoiceSettings，模拟同名环境变量也存在
///   3. VoiceSettingsValues::resolve(saved, env) 合成有效配置
///
/// 预期结果：
///   - 正断言：endpoint / planner / refine 均使用 saved 值
///   - 正断言：trace 使用 saved=false，而不是 env=true
///   - 负断言：任何字段都不应退回 env 值
#[test]
fn test_voice_settings_saved_values_override_env_values() {
    let saved = SavedVoiceSettings {
        ollama_endpoint: Some("http://saved:11434".to_string()),
        planner_model: Some("saved-planner".to_string()),
        refine_model: Some("saved-refine".to_string()),
        input_device_id: Some("saved-headset".to_string()),
        trace_enabled: Some(false),
        ..SavedVoiceSettings::default()
    };
    let env = EnvVoiceSettings {
        ollama_endpoint: Some("http://env:11434".to_string()),
        planner_model: Some("env-planner".to_string()),
        refine_model: Some("env-refine".to_string()),
        input_device_id: Some("env-headset".to_string()),
        trace_enabled: Some("true".to_string()),
        ..EnvVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &env);

    assert_eq!(
        values.ollama_endpoint(),
        "http://saved:11434",
        "已保存 endpoint 应优先于 MULTISOUL_OLLAMA_ENDPOINT"
    );
    assert_eq!(
        values.planner_model(),
        "saved-planner",
        "已保存 planner model 应优先于 MULTISOUL_OLLAMA_MODEL"
    );
    assert_eq!(
        values.refine_model(),
        "saved-refine",
        "已保存 refine model 应优先于 MULTISOUL_REFINE_MODEL"
    );
    assert_eq!(
        values.input_device_id(),
        Some("saved-headset"),
        "已保存 input device id 应优先于 MULTISOUL_VOICE_INPUT_DEVICE_ID"
    );
    assert!(
        !values.trace_enabled(),
        "已保存 trace=false 应优先于 env trace=true"
    );
    assert_ne!(
        values.ollama_endpoint(),
        "http://env:11434",
        "endpoint 不应被 env 覆盖"
    );
}

/// VS-2: 没有保存设置时应使用环境变量，并让 refine model 跟随 planner fallback。
///
/// 数据构造（含关键数值的推导过程）：
///   saved = 全部 None
///   env endpoint = http://env:11434
///   env planner = env-planner
///   env refine = None，因此 refine 应回退到有效 planner
///   env trace = "on"，属于明确 truthy 值
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造空 SavedVoiceSettings
///   2. 构造 EnvVoiceSettings，其中 refine_model 缺失
///   3. resolve 后读取 endpoint / planner / refine / trace
///
/// 预期结果：
///   - 正断言：endpoint / planner 来自 env
///   - 正断言：refine model 等于 planner model
///   - 正断言：trace_enabled 为 true
///   - 负断言：refine 不应退回硬编码默认模型
#[test]
fn test_voice_settings_env_fallback_and_refine_follows_planner() {
    let saved = SavedVoiceSettings::default();
    let env = EnvVoiceSettings {
        ollama_endpoint: Some("http://env:11434".to_string()),
        planner_model: Some("env-planner".to_string()),
        refine_model: None,
        input_device_id: Some("env-headset".to_string()),
        trace_enabled: Some("on".to_string()),
        ..EnvVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &env);

    assert_eq!(
        values.ollama_endpoint(),
        "http://env:11434",
        "缺少保存设置时 endpoint 应来自 env"
    );
    assert_eq!(
        values.planner_model(),
        "env-planner",
        "缺少保存设置时 planner model 应来自 env"
    );
    assert_eq!(
        values.refine_model(),
        "env-planner",
        "refine model 缺失时应跟随有效 planner model"
    );
    assert_eq!(
        values.input_device_id(),
        Some("env-headset"),
        "缺少保存设置时 input device id 应来自 env"
    );
    assert!(values.trace_enabled(), "env trace=on 应被识别为开启");
    assert_ne!(
        values.refine_model(),
        voice::OllamaVoicePlanner::default_model(),
        "refine 不应在 planner env 存在时退回硬编码默认模型"
    );
}

/// VS-3: 保存设置和环境变量都缺失时，应回到当前语音链路默认值。
///
/// 数据构造（含关键数值的推导过程）：
///   saved = 全部 None
///   env = 全部 None
///   default endpoint = http://127.0.0.1:11434
///   default planner = OllamaVoicePlanner::default_model()
///   default refine = default planner
///   trace default = false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve(empty saved, empty env)
///   2. 读取各字段默认值
///
/// 预期结果：
///   - 正断言：endpoint / planner / refine 使用现有默认
///   - 正断言：trace 默认关闭
///   - 负断言：ASR model ID 不应被伪造
#[test]
fn test_voice_settings_defaults_when_saved_and_env_are_missing() {
    let values =
        VoiceSettingsValues::resolve(&SavedVoiceSettings::default(), &EnvVoiceSettings::default());

    assert_eq!(
        values.ollama_endpoint(),
        DEFAULT_OLLAMA_ENDPOINT,
        "缺少保存设置和 env 时 endpoint 应使用当前默认地址"
    );
    assert_eq!(
        values.planner_model(),
        voice::OllamaVoicePlanner::default_model(),
        "缺少保存设置和 env 时 planner model 应使用 voice crate 默认模型"
    );
    assert_eq!(
        values.refine_model(),
        voice::OllamaVoicePlanner::default_model(),
        "缺少保存设置和 env 时 refine model 应跟随默认 planner model"
    );
    assert!(
        !values.trace_enabled(),
        "缺少保存设置和 env 时 trace 应默认关闭"
    );
    assert!(
        values.asr_model_id().is_none(),
        "缺少新模型选择时不应伪造 ASR 模型 ID"
    );
    assert!(
        values.input_device_id().is_none(),
        "缺少保存设置和 env 时不应伪造自定义输入设备"
    );
}

/// VS-4: Settings 中选择输入设备后，应写入当前状态并可清回 System Default。
///
/// 数据构造（含关键数值的推导过程）：
///   initial saved.input_device_id = None
///   selected id = "headset"
///   clear selection = None，表示 System Default
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用空 saved/env 构造 VoiceSettingsState
///   2. select_input_device(Some("headset")) → values.input_device_id 应变为 headset
///   3. select_input_device(None) → values.input_device_id 应回到 None
///
/// 预期结果：
///   - 正断言：选择后读回 headset
///   - 正断言：清除后读回 None
///   - 负断言：清除后不应继续保留旧 headset
#[test]
fn test_voice_settings_select_and_clear_input_device_id() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    state.select_input_device(Some("headset".to_string()));

    assert_eq!(
        state.values().input_device_id(),
        Some("headset"),
        "选择耳机麦克风后 values 应读回 headset"
    );

    state.select_input_device(None);

    assert!(
        state.values().input_device_id().is_none(),
        "清除输入设备后应回到 System Default"
    );
    assert_ne!(
        state.values().input_device_id(),
        Some("headset"),
        "清除输入设备后不应继续保留旧 headset"
    );
}

/// VS-5: 刷新输入设备失败时应保留当前选择，并显示错误状态。
///
/// 数据构造（含关键数值的推导过程）：
///   selected id = "headset"
///   refresh result = Err(SpeechError::Audio("device service unavailable"))
///   devices before refresh = []
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 VoiceSettingsState 并选择 headset
///   2. 调用 refresh_input_devices_from_result(Err(...)) 模拟系统枚举失败
///   3. 读取 values.input_device_id 与 status
///
/// 预期结果：
///   - 正断言：input_device_id 仍为 headset
///   - 正断言：status 标记为 error 且包含失败原因
///   - 负断言：刷新失败不应清空选择
#[test]
fn test_voice_settings_refresh_input_devices_error_preserves_selection() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.select_input_device(Some("headset".to_string()));

    state.refresh_input_devices_from_result(Err(voice_sherpa::SpeechError::Audio(
        "device service unavailable".to_string(),
    )));

    let status = state.status().expect("刷新失败应写入 status");

    assert_eq!(
        state.values().input_device_id(),
        Some("headset"),
        "刷新设备失败时应保留当前选择"
    );
    assert!(status.1, "刷新设备失败时 status 应标记为错误");
    assert!(
        status.0.contains("device service unavailable"),
        "刷新设备失败时 status 应包含底层失败原因"
    );
    assert_ne!(
        state.values().input_device_id(),
        None,
        "刷新设备失败不应清空已选择的输入设备"
    );
}

/// VS-6：Ollama 与 OpenAI profile 应分别保留，并只导出当前选择的 ProviderConfig。
///
/// 数据构造：Ollama 使用本地 URL/model，OpenAI 使用 HTTPS URL/model/key，active=OpenAI。
/// 执行过程：resolve 两套 profile，再读取 active profile 和 agent_provider_config。
/// 预期结果：当前配置为 OpenAI；Ollama 值仍保留；Debug 不包含 API Key。
#[test]
fn test_agent_provider_profiles_are_retained_and_active_profile_is_exported() {
    let saved = SavedVoiceSettings {
        agent_provider: Some(AgentProviderKind::OpenAiCompatible),
        ollama_profile: SavedProviderProfile {
            base_url: Some("http://127.0.0.1:11434".to_string()),
            api_key: Some("ollama-secret".to_string()),
            model: Some("qwen-local".to_string()),
        },
        openai_profile: SavedProviderProfile {
            base_url: Some("https://api.openai.com/v1".to_string()),
            api_key: Some("top-secret".to_string()),
            model: Some("gpt-test".to_string()),
        },
        ..SavedVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    let config = values
        .agent_provider_config()
        .expect("完整 OpenAI profile 应导出 ProviderConfig");
    let debug = format!("{values:?}");

    assert_eq!(
        values.agent_provider(),
        AgentProviderKind::OpenAiCompatible,
        "active provider 应为 OpenAI-compatible"
    );
    assert_eq!(
        config.model(),
        "gpt-test",
        "导出配置应使用当前 OpenAI model"
    );
    assert_eq!(
        values.ollama_profile.model(),
        "qwen-local",
        "切换到 OpenAI 后应继续保留 Ollama model"
    );
    assert!(
        !debug.contains("top-secret"),
        "Voice Settings Debug 不得泄漏 API Key"
    );
    assert!(
        !debug.contains("ollama-secret"),
        "未激活 Ollama profile 的 API Key 也不得出现在 Debug"
    );
}

/// VS-7：OpenAI 默认 profile 不应擅自选择付费模型。
///
/// 数据构造：仅选择 OpenAI，不保存 base_url、model 或 key。
/// 执行过程：resolve 默认值并尝试导出 ProviderConfig。
/// 预期结果：base_url 使用官方 /v1；model 为空；配置校验明确失败。
#[test]
fn test_openai_defaults_base_url_but_requires_explicit_model() {
    let saved = SavedVoiceSettings {
        agent_provider: Some(AgentProviderKind::OpenAiCompatible),
        ..SavedVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    let result = values.agent_provider_config();

    assert_eq!(
        values.active_provider_profile().base_url(),
        super::provider_values::DEFAULT_OPENAI_BASE_URL,
        "OpenAI base_url 应默认到官方 /v1"
    );
    assert_eq!(
        values.active_provider_profile().model(),
        "",
        "OpenAI model 必须由用户显式填写"
    );
    assert!(result.is_err(), "OpenAI model 为空时不得构造生产 Provider");
}

/// VS-8：canonical Provider 设置应从 app SQLite 完整读回且密钥只在专用 getter 可见。
///
/// 数据构造：临时 SettingsStore 写入 active=openai、HTTPS base_url、model 和 top-secret。
/// 执行过程：通过 VoiceSettingsState::load_from_store 读取，再导出 ProviderConfig 和 Debug。
/// 预期结果：配置字段完整；Debug 不含密钥；Ollama profile 没有被 OpenAI 值覆盖。
#[test]
fn test_agent_provider_settings_round_trip_through_sqlite_with_masked_debug() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string(super::provider_values::KEY_AGENT_PROVIDER, "openai")
        .expect("应保存 active provider");
    store
        .set_string(
            super::provider_values::KEY_AGENT_OPENAI_BASE_URL,
            "https://example.test/v1",
        )
        .expect("应保存 OpenAI base_url");
    store
        .set_string(super::provider_values::KEY_AGENT_OPENAI_MODEL, "gpt-test")
        .expect("应保存 OpenAI model");
    store
        .set_string(
            super::provider_values::KEY_AGENT_OPENAI_API_KEY,
            "top-secret",
        )
        .expect("应保存 OpenAI API Key");

    let state = VoiceSettingsState::load_from_store(store);
    let config = state
        .values()
        .agent_provider_config()
        .expect("SQLite 中完整 profile 应导出配置");
    let debug = format!("{state:?}");

    assert_eq!(
        config.endpoint(),
        "https://example.test/v1",
        "应从 SQLite 读回 OpenAI base_url"
    );
    assert_eq!(
        config.api_key(),
        Some("top-secret"),
        "专用 getter 应读回密钥"
    );
    assert!(
        !debug.contains("top-secret"),
        "VoiceSettingsState Debug 不得泄漏 SQLite API Key"
    );
    assert_ne!(
        state.values().ollama_profile.base_url(),
        "https://example.test/v1",
        "OpenAI profile 不得覆盖独立 Ollama profile"
    );
}

/// VS-9：编辑中的 API Key 也不得通过 Debug 或 Settings 行文本泄漏。
///
/// 数据构造：进入 AgentApiKey 编辑态，并模拟草稿值 top-secret。
/// 执行过程：格式化 VoiceSettingsState Debug，并读取 API Key 行展示值。
/// 预期结果：两处都不含明文；行文本只显示与字符数对应的掩码。
#[test]
fn test_agent_api_key_editing_state_is_masked_everywhere() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.begin_edit(VoiceSettingsField::ApiKey);
    state.draft = "top-secret".to_string();

    let debug = format!("{state:?}");
    let row = state.row_value(VoiceSettingsField::ApiKey);

    assert!(
        !debug.contains("top-secret"),
        "编辑态 Debug 不得包含 API Key 草稿"
    );
    assert!(
        !row.contains("top-secret"),
        "Settings 行不得显示 API Key 草稿明文"
    );
    assert_eq!(
        row, "••••••••••|",
        "10 个 API Key 字符应显示为 10 个掩码字符和光标"
    );
}

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
