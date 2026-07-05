use super::values::DEFAULT_OLLAMA_ENDPOINT;
use super::*;
use crate::app::realtime::RealtimeControlState;
use gpui::{Keystroke, Modifiers};
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_realtime_env_vars(assertions: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().expect("env mutex should not be poisoned");
    let keys = [
        "OPENAI_API_KEY",
        "MULTISOUL_OPENAI_API_KEY",
        "MULTISOUL_OPENAI_REALTIME_MODEL",
        "MULTISOUL_OPENAI_CARD_PLANNER_MODEL",
        "MULTISOUL_OPENAI_SIDECAR_PATH",
    ];
    let previous = keys.map(|key| (key, std::env::var(key).ok()));

    std::env::set_var("OPENAI_API_KEY", "sk-env-openai");
    std::env::set_var("MULTISOUL_OPENAI_API_KEY", "sk-env-multisoul");
    std::env::set_var("MULTISOUL_OPENAI_REALTIME_MODEL", "env-realtime-model");
    std::env::set_var("MULTISOUL_OPENAI_CARD_PLANNER_MODEL", "env-planner-model");
    std::env::set_var("MULTISOUL_OPENAI_SIDECAR_PATH", "/env/sidecar");

    assertions();

    for (key, value) in previous {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

fn values_with_realtime_parts(
    openai_api_key: Option<&str>,
    realtime_model: Option<&str>,
) -> VoiceSettingsValues {
    VoiceSettingsValues {
        ollama_endpoint: DEFAULT_OLLAMA_ENDPOINT.to_string(),
        planner_model: voice::OllamaVoicePlanner::default_model().to_string(),
        refine_model: voice::OllamaVoicePlanner::default_model().to_string(),
        paraformer_model_dir: None,
        input_device_id: None,
        trace_enabled: false,
        realtime_enabled: true,
        openai_api_key: openai_api_key.map(ToOwned::to_owned),
        realtime_model: realtime_model.map(ToOwned::to_owned),
        card_planner_model: Some("gpt-card-planner".to_string()),
        sidecar_path: None,
    }
}

fn key(key: &str, key_char: Option<&str>) -> Keystroke {
    Keystroke {
        key: key.to_string(),
        modifiers: Modifiers::default(),
        key_char: key_char.map(str::to_string),
    }
}

/// VS-6: OpenAI realtime 字段只从 Settings saved 值解析，不从环境变量 fallback。
///
/// 数据构造（含关键数值的推导过程）：
///   saved = 全部 None
///   env process = OPENAI_API_KEY / MULTISOUL_OPENAI_* 均设置为非空值
///   default realtime model = gpt-realtime-2（saved model 缺失时的显式默认）
///   realtime enabled default = false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 临时设置 OpenAI 相关环境变量
///   2. EnvVoiceSettings::from_env() 只读取既有 one-shot voice 环境变量
///   3. VoiceSettingsValues::resolve(empty saved, env) 合成有效配置
///   4. 查询 realtime_enabled / openai_api_key / realtime_model / planner / sidecar
///
/// 预期结果：
///   - 正断言：realtime 默认关闭，realtime_model 使用 gpt-realtime-2
///   - 负断言：API key、planner model、sidecar path 不应来自任何 env fallback
#[test]
fn test_realtime_openai_fields_do_not_fallback_to_env() {
    with_realtime_env_vars(|| {
        let saved = SavedVoiceSettings::default();
        let env = EnvVoiceSettings::from_env();

        let values = VoiceSettingsValues::resolve(&saved, &env);

        assert!(
            !values.realtime_enabled(),
            "缺少保存设置时 Always Listening 应默认关闭"
        );
        assert!(
            values.openai_api_key().is_none(),
            "OpenAI API key 不应从 OPENAI_API_KEY 或 MULTISOUL_OPENAI_API_KEY fallback"
        );
        assert_eq!(
            values.realtime_model(),
            Some("gpt-realtime-2"),
            "缺少保存模型时 realtime model 应使用默认 gpt-realtime-2"
        );
        assert!(
            values.card_planner_model().is_none(),
            "Card Planner Model 不应从 MULTISOUL_OPENAI_CARD_PLANNER_MODEL fallback"
        );
        assert!(
            values.sidecar_path().is_none(),
            "Sidecar Path 不应从 MULTISOUL_OPENAI_SIDECAR_PATH fallback"
        );
    });
}

/// VS-7: 保存的 API key、模型、sidecar 路径和 enabled flag 应进入 VoiceSettingsValues。
///
/// 数据构造（含关键数值的推导过程）：
///   saved.realtime_enabled = true
///   saved.openai_api_key = "sk-saved"
///   saved.realtime_model = "gpt-realtime-saved"
///   saved.card_planner_model = "gpt-planner-saved"
///   saved.sidecar_path = /saved/realtime-sidecar
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造带 realtime 字段的 SavedVoiceSettings
///   2. 使用空 EnvVoiceSettings 调用 resolve
///   3. 逐一读取 realtime getters
///
/// 预期结果：
///   - 正断言：五个 saved realtime 字段均可从 values 读回
///   - 负断言：realtime model 不应退回默认 gpt-realtime-2
#[test]
fn test_realtime_saved_values_resolve_into_voice_settings_values() {
    let saved = SavedVoiceSettings {
        realtime_enabled: Some(true),
        openai_api_key: Some("sk-saved".to_string()),
        realtime_model: Some("gpt-realtime-saved".to_string()),
        card_planner_model: Some("gpt-planner-saved".to_string()),
        sidecar_path: Some(PathBuf::from("/saved/realtime-sidecar")),
        ..SavedVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());

    assert!(
        values.realtime_enabled(),
        "保存 enabled=true 后 values 应读回 Always Listening 开启"
    );
    assert_eq!(
        values.openai_api_key(),
        Some("sk-saved"),
        "保存 API key 后 values 应读回同一 key 供启动校验使用"
    );
    assert_eq!(
        values.realtime_model(),
        Some("gpt-realtime-saved"),
        "保存 realtime model 后 values 应读回自定义模型"
    );
    assert_eq!(
        values.card_planner_model(),
        Some("gpt-planner-saved"),
        "保存 planner model 后 values 应读回自定义 planner"
    );
    assert_eq!(
        values.sidecar_path(),
        Some(PathBuf::from("/saved/realtime-sidecar").as_path()),
        "保存 sidecar path 后 values 应读回同一路径"
    );
    assert_ne!(
        values.realtime_model(),
        Some("gpt-realtime-2"),
        "保存 realtime model 后不应继续使用默认模型"
    );
}

/// VS-8: Realtime 启动配置缺少 API key 或模型时应返回配置错误。
///
/// 数据构造（含关键数值的推导过程）：
///   missing_key: api_key=None, model=Some("gpt-realtime-2")
///   missing_model: api_key=Some("sk-validation"), model=None
///   helper 默认提供 card_planner_model=Some("gpt-card-planner")，确保只验证 API key / realtime model
///   tools = []（Task 2 shell 不注册真实工具）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对 missing_key 调用 validate_start_config
///   2. 对 missing_model 调用 validate_start_config
///   3. 分别读取错误 Debug/Display 文本
///
/// 预期结果：
///   - 正断言：两种缺失都返回 Err
///   - 负断言：错误 Debug/Display 不应包含 API key
#[test]
fn test_realtime_validate_start_config_requires_api_key_and_model() {
    let missing_key = values_with_realtime_parts(None, Some("gpt-realtime-2"));
    let missing_key_error = RealtimeControlState::validate_start_config(&missing_key)
        .expect_err("缺少 OpenAI API key 时应返回配置错误");

    let missing_model = values_with_realtime_parts(Some("sk-validation"), None);
    let missing_model_error = RealtimeControlState::validate_start_config(&missing_model)
        .expect_err("缺少 realtime model 时应返回配置错误");

    assert!(
        format!("{missing_key_error:?}").contains("MissingApiKey"),
        "缺少 API key 时 Debug 应说明 MissingApiKey，实际: {missing_key_error:?}"
    );
    assert!(
        format!("{missing_model_error:?}").contains("MissingRealtimeModel"),
        "缺少 model 时 Debug 应说明 MissingRealtimeModel，实际: {missing_model_error:?}"
    );
    assert!(
        !format!("{missing_model_error:?}").contains("sk-validation"),
        "validation error Debug 不应泄露 API key"
    );
    assert!(
        !missing_model_error.to_string().contains("sk-validation"),
        "validation error Display 不应泄露 API key"
    );
}

/// VS-8b: Realtime 启动配置缺少 Card Planner Model 时也应返回配置错误。
///
/// 数据构造（含关键数值的推导过程）：
///   api_key = Some("sk-planner")
///   realtime_model = Some("gpt-realtime-2")
///   card_planner_model = None（唯一缺失字段）
///   tools = []（Task 2 shell 不注册真实工具）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只缺 card_planner_model 的 VoiceSettingsValues
///   2. 调用 validate_start_config
///   3. 检查错误 Debug/Display 是否不包含 API key
///
/// 预期结果：
///   - 正断言：返回 MissingCardPlannerModel
///   - 负断言：错误文本不应泄露 API key，也不应误报 MissingApiKey
#[test]
fn test_realtime_validate_start_config_requires_card_planner_model() {
    let missing_planner = VoiceSettingsValues {
        card_planner_model: None,
        ..values_with_realtime_parts(Some("sk-planner"), Some("gpt-realtime-2"))
    };

    let error = RealtimeControlState::validate_start_config(&missing_planner)
        .expect_err("缺少 Card Planner Model 时应返回配置错误");

    assert!(
        format!("{error:?}").contains("MissingCardPlannerModel"),
        "缺少 planner model 时 Debug 应说明 MissingCardPlannerModel，实际: {error:?}"
    );
    assert!(
        !format!("{error:?}").contains("sk-planner"),
        "planner 缺失错误 Debug 不应泄露 API key"
    );
    assert!(
        !error.to_string().contains("sk-planner"),
        "planner 缺失错误 Display 不应泄露 API key"
    );
    assert!(
        !format!("{error:?}").contains("MissingApiKey"),
        "planner 缺失时不应误报 MissingApiKey"
    );
}

/// VS-9: saved realtime model 缺失时，Values 应使用 gpt-realtime-2 作为默认模型。
///
/// 数据构造（含关键数值的推导过程）：
///   saved.realtime_model = None
///   saved.openai_api_key = Some("sk-default-model")
///   saved.card_planner_model = Some("gpt-card-planner")（保证只有 model default 被测试）
///   expected realtime_model = gpt-realtime-2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只保存 API key 的 SavedVoiceSettings
///   2. resolve 后读取 realtime_model
///   3. validate_start_config 应因默认模型存在而成功
///
/// 预期结果：
///   - 正断言：realtime_model 返回 Some("gpt-realtime-2")
///   - 正断言：启动配置 model 也是 gpt-realtime-2
///   - 负断言：realtime_model 不应为 None
#[test]
fn test_realtime_model_defaults_to_gpt_realtime_2_when_saved_model_is_missing() {
    let saved = SavedVoiceSettings {
        openai_api_key: Some("sk-default-model".to_string()),
        card_planner_model: Some("gpt-card-planner".to_string()),
        ..SavedVoiceSettings::default()
    };

    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    let config = RealtimeControlState::validate_start_config(&values)
        .expect("保存 API key 且缺少模型时应使用默认 gpt-realtime-2 并通过校验");

    assert_eq!(
        values.realtime_model(),
        Some("gpt-realtime-2"),
        "saved realtime_model 缺失时 values 应使用默认 gpt-realtime-2"
    );
    assert_eq!(
        config.model, "gpt-realtime-2",
        "启动配置应继承 values 中的默认 realtime model"
    );
    assert_ne!(
        values.realtime_model(),
        None,
        "saved realtime_model 缺失不应让 values.realtime_model() 返回 None"
    );
}

/// VS-11: OpenAI API Key 编辑态提交空值应清除已保存 key，而不是卡在错误状态。
///
/// 数据构造（含关键数值的推导过程）：
///   saved.openai_api_key = Some("sk-clear")
///   begin_edit(OpenAiApiKey) 按安全规则展示空 draft，不把旧 key 放入输入框
///   key("enter") 表示用户直接提交空输入，目标是清除 key
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造带已保存 API key 的 VoiceSettingsState
///   2. begin_edit(OpenAiApiKey) 后直接 handle_key_down(enter)
///   3. 读取 values.openai_api_key() / row_value(OpenAiApiKey) / Debug 文本
///
/// 预期结果：
///   - 正断言：提交空值后 API key 被清除，row 显示 Not set
///   - 负断言：旧 key 不应继续存在于 values 或 Debug 输出
#[test]
fn test_realtime_openai_api_key_empty_commit_clears_saved_key() {
    let saved = SavedVoiceSettings {
        openai_api_key: Some("sk-clear".to_string()),
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());

    state.begin_edit(VoiceSettingsField::OpenAiApiKey);
    let handled = state.handle_key_down(&key("enter", None));

    assert!(handled, "OpenAI API key 编辑态应处理 enter 提交");
    assert!(
        state.values().openai_api_key().is_none(),
        "OpenAI API key 空提交后应清除保存值"
    );
    assert_eq!(
        state.row_value(VoiceSettingsField::OpenAiApiKey),
        "Not set",
        "API key 清除后 Settings 行应显示 Not set"
    );
    assert_ne!(
        state.values().openai_api_key(),
        Some("sk-clear"),
        "API key 清除后 values 不应继续保留旧 key"
    );
    assert!(
        !format!("{state:?}").contains("sk-clear"),
        "API key 清除后 Debug 输出不应包含旧 key"
    );
}

/// VS-10: API key 不得出现在 VoiceSettingsValues Debug 或 validation error 文本中。
///
/// 数据构造（含关键数值的推导过程）：
///   secret = "sk-debug-secret"
///   values_with_key = api_key=Some(secret), model=Some("gpt-realtime-2")
///   missing_model = api_key=Some(secret), model=None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. format!("{values_with_key:?}") 获取 Values Debug
///   2. 对 missing_model 调用 validate_start_config 获取错误
///   3. 检查 Debug/Display 文本是否包含 secret 或 masked token
///
/// 预期结果：
///   - 正断言：Values Debug 显示掩码 ••••••••
///   - 负断言：Values Debug、error Debug、error Display 均不包含原始 API key
#[test]
fn test_realtime_api_key_is_masked_in_debug_and_validation_errors() {
    let values_with_key =
        values_with_realtime_parts(Some("sk-debug-secret"), Some("gpt-realtime-2"));
    let values_debug = format!("{values_with_key:?}");

    let missing_model = values_with_realtime_parts(Some("sk-debug-secret"), None);
    let error = RealtimeControlState::validate_start_config(&missing_model)
        .expect_err("缺少 realtime model 应返回错误，且错误不应携带 API key");

    assert!(
        values_debug.contains("••••••••"),
        "Values Debug 有 API key 时应显示固定掩码"
    );
    assert!(
        !values_debug.contains("sk-debug-secret"),
        "Values Debug 不应泄露原始 API key"
    );
    assert!(
        !format!("{error:?}").contains("sk-debug-secret"),
        "validation error Debug 不应泄露原始 API key"
    );
    assert!(
        !error.to_string().contains("sk-debug-secret"),
        "validation error Display 不应泄露原始 API key"
    );
}
