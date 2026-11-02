use crate::settings::voice::{
    EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState, VoiceSetupDestination,
};

/// 场景：旧配置已选本地 A1，合并后的本地入口仍应展示这一链路。
/// 构造：直接读取旧 provider 值，不加载真实模型或凭据。
/// 执行：打开统一的本地来源区域。
/// 预期：浏览入口不会把 A1 换成 Sherpa，也不显示错误的本地详情。
#[test]
fn merged_local_entry_preserves_the_existing_engine() {
    let saved = SavedVoiceSettings {
        asr_provider: Some(asr::AsrProviderId::LocalA1),
        ..Default::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
    state.open_setup_destination(VoiceSetupDestination::AsrOnDevice);
    assert_eq!(
        state.setup_asr_source(),
        asr::AsrProviderId::LocalA1,
        "统一本地入口应保持当前 A1 的详情"
    );
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalA1,
        "浏览本地入口不能改变已保存引擎"
    );
}

/// 场景：本地 A1 用户暂时切到 Azure，再切回本地；期间可以重启。
/// 构造：隔离的已就绪云配置及 A1 文件校验状态，不访问任何网络。
/// 执行：选择云端、保存快照、重新加载、校验并回到本地。
/// 预期：恢复先前 A1 选择，不默认降级 Sherpa。
#[test]
fn returning_from_cloud_restores_the_last_local_engine_after_reload() {
    let saved = SavedVoiceSettings {
        asr_provider: Some(asr::AsrProviderId::LocalA1),
        azure_asr_ready: Some(true),
        azure_asr_region: Some("eastus".into()),
        azure_asr_locale: Some("zh-CN".into()),
        azure_asr_credential_ref: Some("fixture-azure".into()),
        ..Default::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
    assert!(
        state.activate_azure_asr(),
        "已验证的隔离 Azure 配置应可选择"
    );
    let mut reloaded =
        VoiceSettingsState::from_sources(state.saved_snapshot(), EnvVoiceSettings::default());
    let generation = reloaded.begin_a1_validation();
    assert!(
        reloaded.finish_a1_validation(generation, Ok(())),
        "只模拟文件校验完成，不加载真实模型"
    );
    assert!(
        reloaded.activate_local_recognition(),
        "已就绪的本地偏好应可恢复"
    );
    assert_eq!(
        reloaded.values().asr_provider(),
        asr::AsrProviderId::LocalA1,
        "切回本地应恢复 A1，不能静默改成兼容引擎"
    );
}

/// 场景：同一个本地入口在持续聆听中使用兼容引擎，回单次恢复 A1。
/// 构造：已保存 A1，不连接网络、不加载模型。
/// 执行：切持续、保存重载、切单次。
/// 预期：持续不启用 A1；偏好保留，单次可恢复原引擎。
#[test]
fn continuous_mode_uses_compatible_engine_without_losing_a1_preference() {
    use crate::settings::voice::VoiceInteractionMode;
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            asr_provider: Some(asr::AsrProviderId::LocalA1),
            ..Default::default()
        },
        EnvVoiceSettings::default(),
    );
    state.select_interaction_mode(VoiceInteractionMode::ContinuousListening);
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "持续聆听必须使用兼容引擎，不能误启 A1"
    );
    let mut reloaded =
        VoiceSettingsState::from_sources(state.saved_snapshot(), EnvVoiceSettings::default());
    assert_eq!(
        reloaded.preferred_local_provider(),
        asr::AsrProviderId::LocalA1,
        "兼容运行不能覆盖已有偏好"
    );
    reloaded.select_interaction_mode(VoiceInteractionMode::SingleVoiceCommand);
    assert_eq!(
        reloaded.values().asr_provider(),
        asr::AsrProviderId::LocalA1,
        "回单次恢复 A1，仍由原就绪门槛守住"
    );
}

/// 场景：A1 用户在云端期间切成持续聆听，再恢复本地来源。
/// 构造：已验证云配置与保存的 A1 本地偏好。
/// 执行：选择云端、持续模式、统一的本地入口。
/// 预期：可恢复兼容本地运行，A1 偏好仍可在单次模式恢复。
#[test]
fn cloud_to_local_continuous_restores_compatibility_and_preserves_preference() {
    use crate::settings::voice::VoiceInteractionMode;
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            asr_provider: Some(asr::AsrProviderId::LocalA1),
            azure_asr_ready: Some(true),
            azure_asr_region: Some("eastus".into()),
            azure_asr_locale: Some("zh-CN".into()),
            azure_asr_credential_ref: Some("fixture".into()),
            ..Default::default()
        },
        EnvVoiceSettings::default(),
    );
    assert!(state.activate_azure_asr(), "已验证云配置可选择");
    state.select_interaction_mode(VoiceInteractionMode::ContinuousListening);
    assert!(
        state.activate_local_recognition(),
        "持续聆听切回本地应选择兼容运行"
    );
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "持续使用兼容引擎"
    );
    assert_eq!(
        state.preferred_local_provider(),
        asr::AsrProviderId::LocalA1,
        "不能覆盖 A1 偏好"
    );
}
