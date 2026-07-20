use super::*;

/// TTS-ACTIVATION-1：Apply 进行中必须锁定精确 Provider/Model/Voice Draft。
///
/// 数据构造（含关键数值的推导过程）：
///   applying tuple = On-device + Matcha + default
///   competing edits = 2（切到 MiniMax；改为 Kokoro + af_heart）
///   accepted tuples = 1（仅原 Matcha/default）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 Matcha/default 打开统一选择器并进入 applying 状态。
///   2. 模拟持久化期间点击 MiniMax，再尝试写入 Kokoro/af_heart。
///   3. 读取最终 Draft，并分别校验原 tuple 与竞争 tuple 的完成资格。
///
/// 预期结果：
///   - 正断言：Draft 继续保持 applying，原 Matcha tuple 可被完成回调接纳。
///   - 负断言：Provider 不得切到 MiniMax，Kokoro tuple 不得覆盖或获得完成资格。
#[test]
fn test_tts_picker_locks_exact_draft_while_activation_is_applying() {
    let local = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let minimax = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("MiniMax Provider ID 必须有效");
    let matcha =
        tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID).expect("Matcha model ID 必须有效");
    let default_voice = tts::TtsVoiceId::parse("default").expect("默认 Voice ID 必须有效");
    let kokoro =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let heart = tts::TtsVoiceId::parse("af_heart").expect("Kokoro Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(matcha.clone()),
            tts_voice_id: Some(default_voice.clone()),
            active_tts_provider_id: Some(local.clone()),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    state.open_tts_picker();
    state.begin_tts_picker_apply();

    state.select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
    state.set_tts_picker_local_selection(kokoro.clone(), heart.clone());

    assert!(
        state.tts_picker_apply_pending(),
        "竞争编辑不得结束正在进行的 TTS activation"
    );
    assert_eq!(
        state.tts_picker_provider_id(),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "applying 期间 Provider 必须锁定为 On-device"
    );
    assert_eq!(
        state.tts_picker_local_model_id(),
        Some(&matcha),
        "applying 期间 Matcha model 不得被 Kokoro 覆盖"
    );
    assert_eq!(
        state.tts_picker_local_voice_id(),
        Some(&default_voice),
        "applying 期间 default Voice 不得被 af_heart 覆盖"
    );
    assert!(
        state.tts_picker_accepts_activation(&local, &matcha, &default_voice),
        "完成回调必须接纳仍精确匹配的原 Matcha tuple"
    );
    assert!(
        !state.tts_picker_accepts_activation(&minimax, &kokoro, &heart),
        "完成回调不得接纳 applying 期间被拒绝的竞争 tuple"
    );
    assert_ne!(
        state.tts_picker_provider_id(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "applying 期间不得把 Provider 偷换为 MiniMax"
    );
}
