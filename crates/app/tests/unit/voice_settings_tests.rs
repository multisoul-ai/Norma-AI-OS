use super::values::DEFAULT_OLLAMA_ENDPOINT;
use super::*;
use std::path::PathBuf;

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
///   - 正断言：endpoint / planner / refine / model_dir 均使用 saved 值
///   - 正断言：trace 使用 saved=false，而不是 env=true
///   - 负断言：任何字段都不应退回 env 值
#[test]
fn test_voice_settings_saved_values_override_env_values() {
    let saved = SavedVoiceSettings {
        ollama_endpoint: Some("http://saved:11434".to_string()),
        planner_model: Some("saved-planner".to_string()),
        refine_model: Some("saved-refine".to_string()),
        paraformer_model_dir: Some(PathBuf::from("/saved/model")),
        input_device_id: Some("saved-headset".to_string()),
        trace_enabled: Some(false),
        ..SavedVoiceSettings::default()
    };
    let env = EnvVoiceSettings {
        ollama_endpoint: Some("http://env:11434".to_string()),
        planner_model: Some("env-planner".to_string()),
        refine_model: Some("env-refine".to_string()),
        paraformer_model_dir: Some(PathBuf::from("/env/model")),
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
        values.paraformer_model_dir(),
        Some(PathBuf::from("/saved/model").as_path()),
        "已保存 Paraformer 目录应优先于 MULTISOUL_PARAFORMER_MODEL_DIR"
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
        paraformer_model_dir: None,
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
///   - 负断言：Paraformer model_dir 不应被伪造成自定义路径
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
        values.paraformer_model_dir().is_none(),
        "缺少保存设置和 env 时不应伪造自定义 Paraformer 目录"
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
