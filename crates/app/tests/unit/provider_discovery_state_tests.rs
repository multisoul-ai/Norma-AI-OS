use super::super::{
    EnvVoiceSettings, ProviderDiscoveryStatus, SavedVoiceSettings, VoiceSettingsState,
};
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

/// Provider 目录失败回退：只有目录能力缺失才开放手动模型，并且验证成功前保持未就绪
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 硬门槛数量 = 1
///   模型目录结果 = 0 models（ModelListUnavailable）
///   手动模型 = custom-model-v1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 fresh Voice state 并记录“模型目录不可用”分类错误
///   2. 检查 UI 状态进入 ManualRequired，但 Provider 门槛仍为 Failed
///   3. 模拟后台对手动模型的真实请求验证成功并记录结果
///
/// 预期结果：
///   - 正断言：目录失败展示手动回退，验证后 Provider 变为 Ready
///   - 负断言：仅填写模型名不能绕过验证提前 Ready
#[test]
fn manual_provider_model_requires_explicit_validation_before_ready() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.record_provider_catalog_error(agent::ProviderCatalogError::ModelListUnavailable(
        "model endpoint unavailable".to_string(),
    ));

    assert!(
        matches!(
            state.provider_discovery(),
            ProviderDiscoveryStatus::ManualRequired(message)
                if message == "model endpoint unavailable"
        ),
        "model-list-specific failure must expose the manual model fallback"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Failed,
        "manual model entry must remain unready until a real provider request validates it"
    );
    assert_ne!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Ready,
        "typing a manual model id must not bypass provider validation"
    );

    state.record_manual_provider_model_validated("custom-model-v1".to_string());

    assert_eq!(
        state.provider_discovery(),
        &ProviderDiscoveryStatus::ManualValidated("custom-model-v1".to_string()),
        "successful manual validation must preserve which exact model was verified"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Ready,
        "successful manual model request must satisfy the Provider readiness gate"
    );
}

/// Provider 模型目录成功：目录成员资格不能替代真实聊天请求验证
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 返回模型数 = 2
///   用户选择模型数 = 1（gpt-test）
///   真实验证调用数 = 0（本测试只覆盖目录与选择状态）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 记录 `/models` 返回的两个模型
///   2. 从目录中选择 gpt-test 并保存为当前模型
///   3. 在真实验证结果尚未返回前读取 Provider 硬门槛
///
/// 预期结果：
///   - 正断言：目录和选择结果被保留，便于后续发起真实验证
///   - 负断言：仅列出或选择模型均不得把 Provider 标记为 Ready
#[test]
fn discovered_model_stays_unready_until_real_request_validation() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    state.record_provider_models(vec!["gpt-other".to_string(), "gpt-test".to_string()]);
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Missing,
        "loading a provider model catalog must not satisfy the validation gate"
    );

    state.select_discovered_provider_model("gpt-test".to_string());

    assert!(
        matches!(state.provider_discovery(), ProviderDiscoveryStatus::Models(models) if models.contains(&"gpt-test".to_string())),
        "selected discovered model must remain available while real validation starts"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Missing,
        "selecting a listed model without a chat request must remain unready"
    );
    assert_ne!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Ready,
        "model-list membership must never be treated as proof that generation works"
    );
}

/// Provider 异步代次：旧连接结果不得覆盖用户发起的新配置
///
/// 数据构造（含关键数值的推导过程）：
///   首次 generation = N
///   重置后 generation = N + 1（wrapping_add）
///   可接受旧结果数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 开始第一次 Provider discovery 并保存 generation
///   2. 模拟用户编辑配置，使 discovery 重置并推进 generation
///   3. 分别检查旧、新 generation 的接收条件
///
/// 预期结果：
///   - 正断言：当前 generation 可被接收
///   - 负断言：第一次异步结果必须被拒绝，不能覆盖新配置
#[test]
fn stale_provider_discovery_generation_is_rejected_after_reset() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let stale = state.begin_provider_discovery();
    state.invalidate_provider_requirement();
    let current = state.begin_provider_discovery();

    assert!(
        state.accepts_provider_discovery_generation(current),
        "latest Provider discovery generation must accept its own result"
    );
    assert!(
        !state.accepts_provider_discovery_generation(stale),
        "stale Provider discovery result must be rejected after configuration reset"
    );
    assert_ne!(
        stale, current,
        "resetting Provider discovery must advance the generation token"
    );
}
