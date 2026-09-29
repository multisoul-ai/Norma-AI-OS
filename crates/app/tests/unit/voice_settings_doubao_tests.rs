use super::*;

/// VS-D1：升级后仍选择原语音链路，单次／连续模式不被豆包选择覆盖。
///
/// 数据构造（含关键数值的推导过程）：空设置代表升级前没有顶层模式键；
/// 连续聆听是旧链路内部的一个已保存值，没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析空设置 → 顶层模式使用旧链路默认值。
///   2. 保存豆包顶层模式和连续聆听内部模式 → 重新解析设置。
///
/// 预期结果：默认不是豆包；切到豆包后，旧的连续聆听配置仍保留。
#[test]
fn test_desktop_voice_mode_keeps_legacy_interaction_setting() {
    let mut saved = SavedVoiceSettings::default();
    let initial = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    assert_eq!(
        initial.desktop_mode(),
        DesktopVoiceMode::Legacy,
        "升级默认必须保持原语音链路"
    );
    assert_ne!(
        initial.desktop_mode(),
        DesktopVoiceMode::Doubao,
        "升级默认不能自行启动豆包"
    );
    saved.desktop_mode = Some(DesktopVoiceMode::Doubao);
    saved.interaction_mode = Some(VoiceInteractionMode::ContinuousListening);
    let selected = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
    assert_eq!(
        selected.desktop_mode(),
        DesktopVoiceMode::Doubao,
        "保存的顶层选择应恢复豆包"
    );
    assert_eq!(
        selected.interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "顶层切换不能抹掉原链路的连续聆听选择"
    );
}

/// VS-D2：豆包凭据只写 Keychain 队列，设置及 Debug 中只留非敏感引用。
///
/// 数据构造（含关键数值的推导过程）：1 条合成密钥；1 个固定 Keychain account；
/// 没有 ASR、TTS 或 Agent Provider 配置，因此豆包与旧链路的就绪判断分离。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 选择豆包并输入密钥，触发自动保存。
///   2. 检查待写队列、设置投影与 Debug 输出。
///   3. 清除密钥并检查设置投影。
///
/// 预期结果：密钥仅进入 Keychain 写任务；设置不含明文，清除后引用消失。
#[test]
fn test_doubao_credential_is_keychain_only_and_clearable() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    state.begin_edit(VoiceSettingsField::DoubaoApiKey);
    let saved = state.auto_save_edit_value(
        VoiceSettingsField::DoubaoApiKey,
        "synthetic-secret-123".into(),
    );
    assert_eq!(
        saved.as_deref(),
        Some(""),
        "密钥输入保存后不能把明文传回输入组件"
    );
    assert_eq!(
        state.values().doubao_credential_ref(),
        Some("doubao-desktop"),
        "配置投影应只保存 Keychain account"
    );
    assert!(
        !format!("{state:?}").contains("synthetic-secret-123"),
        "Debug 不得输出豆包密钥"
    );
    let writes = state.take_pending_writes();
    assert!(writes.iter().any(|write| matches!(write, PendingVoiceSettingWrite::StoreCredential { account, secret } if account == "doubao-desktop" && secret == "synthetic-secret-123")), "密钥应只经 Keychain 写任务落地");
    assert!(!writes.iter().any(|write| matches!(write, PendingVoiceSettingWrite::Set { value, .. } if value == "synthetic-secret-123")), "SQLite 设置不得保存密钥明文");
    state.record_persistence_result(Ok(()));
    state.clear_doubao_credential();
    assert!(
        state.values().doubao_credential_ref().is_none(),
        "清除后豆包凭据引用必须消失"
    );
    assert_ne!(
        state.values().doubao_credential_ref(),
        Some("doubao-desktop"),
        "清除后不能继续显示已配置"
    );
}

/// VS-D3：豆包模式不显示旧 TTS 扫描的“只能文本回复”提示。
///
/// 数据构造（含关键数值的推导过程）：一个未安装的旧 TTS 模型对应一条旧链路状态；
/// 两种顶层模式各运行一次扫描回调，没有预算或阈值参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 选择豆包后模拟旧 TTS 扫描状态 → 保留豆包设置的保存反馈。
///   2. 切回旧链路后再次报告相同扫描状态 → 显示旧 TTS 提示。
///   3. 再切到豆包 → 清掉旧模式留下的状态提示。
///
/// 预期结果：豆包不误报或保留旧 TTS 提示；旧模式继续提供原有缺模型提示。
#[test]
fn test_doubao_mode_ignores_legacy_tts_status() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    state.report_tts_status(
        "No Local TTS model is selected; replies will stay text-only.",
        false,
    );
    assert!(
        !state
            .status()
            .is_some_and(|(message, _)| message.contains("Local TTS")),
        "豆包模式不能显示旧 TTS 缺失提示"
    );
    state.select_desktop_mode(DesktopVoiceMode::Legacy);
    state.report_tts_status(
        "No Local TTS model is selected; replies will stay text-only.",
        false,
    );
    assert!(
        state
            .status()
            .is_some_and(|(message, _)| message.contains("Local TTS")),
        "旧模式仍需显示本地 TTS 状态"
    );
    state.select_desktop_mode(DesktopVoiceMode::Doubao);
    assert_eq!(
        state.status().map(|(message, _)| message),
        Some("Saved for this session"),
        "切换到豆包应显示本次设置反馈，而不是旧 TTS 提示"
    );
}
