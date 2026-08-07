use super::*;

/// TTS-PICKER-1：选择器中的候选 Provider、Model 与 Voice 必须与当前生效配置隔离。
///
/// 数据构造（含关键数值的推导过程）：
///   providers = 2（当前 On-device + 候选 MiniMax）
///   local active tuple = Kokoro + af_bella
///   saved MiniMax tuple = speech-2.8-turbo + saved_voice + Catalog
///   draft MiniMax tuple = speech-2.8-hd + candidate_voice + Manual
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 On-device 为 active，并预存一份可恢复的 MiniMax Profile。
///   2. 打开选择器，切换候选 Provider 到 MiniMax，再修改候选 Model 与 Voice。
///   3. 读取 active 与已保存 Profile，确认候选编辑尚未泄漏。
///   4. 取消选择器，再次读取 active、已保存 Profile 与临时草稿。
///
/// 预期结果：
///   - 正断言：选择器内可读取新的 MiniMax 候选 tuple。
///   - 正断言：取消前后 active 均保持 On-device，已保存 MiniMax tuple 保持原值。
///   - 负断言：候选 HD/candidate_voice 不得写入持久 Profile；取消后不得残留草稿。
#[test]
fn test_tts_picker_cancel_discards_draft_without_changing_active_profiles() {
    let local_provider = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let local_model =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let local_voice = tts::TtsVoiceId::parse("af_bella").expect("本地 Voice ID 必须有效");
    let saved_model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let saved_voice =
        tts::TtsVoiceId::parse("saved_voice").expect("已保存 MiniMax Voice ID 必须有效");
    let draft_model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_HD_MODEL_ID)
        .expect("MiniMax HD model ID 必须有效");
    let draft_voice =
        tts::TtsVoiceId::parse("candidate_voice").expect("候选 MiniMax Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(local_model),
            tts_voice_id: Some(local_voice),
            active_tts_provider_id: Some(local_provider.clone()),
            minimax_tts_model_id: Some(saved_model.clone()),
            minimax_tts_voice_id: Some(saved_voice.clone()),
            minimax_tts_voice_provenance: Some(tts::VoiceSelectionProvenance::Catalog),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );

    state.open_tts_picker();
    state.select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
    state.set_tts_picker_minimax_selection(
        draft_model.clone(),
        draft_voice.clone(),
        tts::VoiceSelectionProvenance::Manual,
    );

    assert_eq!(
        state.tts_picker_provider_id(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "选择器必须显示用户正在编辑的 MiniMax Provider"
    );
    assert_eq!(
        state.tts_picker_minimax_model_id(),
        Some(&draft_model),
        "选择器必须保留候选 MiniMax HD model"
    );
    assert_eq!(
        state.tts_picker_minimax_voice_id(),
        Some(&draft_voice),
        "选择器必须保留候选 MiniMax Voice"
    );
    assert_eq!(
        state.values().active_tts_provider_id(),
        Some(&local_provider),
        "编辑候选配置时 active Provider 必须继续保持 On-device"
    );
    assert_eq!(
        state.values().minimax_tts_model_id(),
        Some(&saved_model),
        "候选 HD model 不得提前覆盖已保存的 Turbo Profile"
    );
    assert_eq!(
        state.values().minimax_tts_voice_id(),
        Some(&saved_voice),
        "候选 Voice 不得提前覆盖已保存的 MiniMax Voice"
    );

    state.cancel_tts_picker();

    assert!(!state.tts_picker_open(), "取消后统一 TTS 选择器必须关闭");
    assert_eq!(
        state.values().active_tts_provider_id(),
        Some(&local_provider),
        "取消候选配置不得改变 active On-device Provider"
    );
    assert_eq!(
        state.values().minimax_tts_model_id(),
        Some(&saved_model),
        "取消后已保存 MiniMax model 必须保持 Turbo"
    );
    assert_eq!(
        state.values().minimax_tts_voice_id(),
        Some(&saved_voice),
        "取消后已保存 MiniMax Voice 必须保持原值"
    );
    assert_eq!(
        state.tts_picker_provider_id(),
        None,
        "取消后不得残留候选 Provider"
    );
    assert_ne!(
        state.values().minimax_tts_voice_id(),
        Some(&draft_voice),
        "取消后候选 Voice 不得泄漏到已保存 Profile"
    );
}

/// TTS-PICKER-4：取消选择器必须丢弃尚未 Connect 的 MiniMax API Key 候选。
///
/// 数据构造（含关键数值的推导过程）：
///   connected accounts = 1（saved-account）
///   credential candidates = 1（candidate-key）
///   successful Connect actions = 0（候选从未验证）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以已连接账户打开选择器并切换到 MiniMax。
///   2. 输入 replacement API Key，但不点击 Connect。
///   3. 点击 Cancel 后检查候选、已连接账户和账户状态。
///
/// 预期结果：
///   - 正断言：已连接账户与旧 secret 继续保留，状态仍为 Ready。
///   - 负断言：未验证 candidate-key 不得残留，也不得替换 saved-account。
#[test]
fn test_tts_picker_cancel_discards_unconnected_minimax_credential_candidate() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            minimax_tts_credential_ref: Some("saved-account".to_string()),
            minimax_tts_api_key: Some("saved-secret".to_string()),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("已保存的测试账户必须允许建立 Ready 状态");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "当前连接结果必须被接纳"
    );
    state.open_tts_picker();
    state.select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
    state.update_minimax_api_key_draft("candidate-key".to_string());

    assert!(
        state.minimax_credential_draft_pending(),
        "取消前应存在未 Connect 的 API Key 候选"
    );
    state.cancel_tts_picker();

    assert!(
        !state.minimax_credential_draft_pending(),
        "Cancel 必须丢弃未 Connect 的 API Key 候选"
    );
    assert_eq!(
        state.values().minimax_tts_credential_ref(),
        Some("saved-account"),
        "Cancel 后已连接账户引用必须保持不变"
    );
    assert_eq!(
        state.values().minimax_tts_api_key(),
        Some("saved-secret"),
        "Cancel 后已连接账户 secret 必须保持不变"
    );
    assert_eq!(
        state.minimax_account_status(),
        tts::RemoteAccountStatus::Ready,
        "Cancel 后已连接账户必须继续保持 Ready"
    );
    assert_ne!(
        state.values().minimax_tts_api_key(),
        Some("candidate-key"),
        "未验证 candidate-key 不得替换已连接账户"
    );
}

/// TTS-PICKER-5：激活失败回滚只能恢复 TTS tuple，不得覆盖并发修改的其他 Voice 设置。
///
/// 数据构造（含关键数值的推导过程）：
///   old TTS tuple = Matcha/default + On-device
///   candidate TTS tuple = Kokoro/af_heart + MiniMax（模拟激活期间的内存变更）
///   concurrent non-TTS changes = 1（Interaction: SingleVoiceCommand → ContinuousListening）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 捕获只属于 TTS 激活的 rollback 快照。
///   2. 修改候选 TTS tuple，同时模拟用户修改 Interaction。
///   3. 执行 TTS rollback，再读取 TTS 与 Interaction 的最终值。
///
/// 预期结果：
///   - 正断言：旧 Matcha/default 与 On-device Provider 被精确恢复。
///   - 正断言：并发修改的 ContinuousListening 继续保留。
///   - 负断言：候选 Kokoro/af_heart 不得残留，Interaction 不得退回 SingleVoiceCommand。
#[test]
fn test_tts_activation_rollback_preserves_concurrent_non_tts_changes() {
    let matcha =
        tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID).expect("Matcha model ID 必须有效");
    let default_voice = tts::TtsVoiceId::parse("default").expect("默认 Voice ID 必须有效");
    let kokoro =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let heart = tts::TtsVoiceId::parse("af_heart").expect("Kokoro Voice ID 必须有效");
    let local = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let minimax = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("MiniMax Provider ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            interaction_mode: Some(VoiceInteractionMode::SingleVoiceCommand),
            tts_model_id: Some(matcha.clone()),
            tts_voice_id: Some(default_voice.clone()),
            active_tts_provider_id: Some(local.clone()),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    let rollback = state.capture_tts_activation_rollback();
    state.select_tts_model_and_voice(Some(kokoro.clone()), Some(heart.clone()));
    state.select_tts_provider(Some(minimax));
    state.select_interaction_mode(VoiceInteractionMode::ContinuousListening);

    state.restore_tts_activation(rollback);

    assert_eq!(
        state.values().tts_model_id(),
        Some(&matcha),
        "TTS rollback 必须恢复旧 Matcha model"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&default_voice),
        "TTS rollback 必须恢复旧 default Voice"
    );
    assert_eq!(
        state.values().active_tts_provider_id(),
        Some(&local),
        "TTS rollback 必须恢复旧 On-device Provider"
    );
    assert_eq!(
        state.values().interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "TTS rollback 不得覆盖并发修改的 Interaction"
    );
    assert_ne!(
        state.values().tts_model_id(),
        Some(&kokoro),
        "TTS rollback 后不得残留候选 Kokoro model"
    );
    assert_ne!(
        state.values().tts_voice_id(),
        Some(&heart),
        "TTS rollback 后不得残留候选 Kokoro Voice"
    );
    assert_ne!(
        state.values().interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "TTS rollback 不得把 Interaction 退回旧 SingleVoiceCommand"
    );
}

/// TTS-PICKER-2：已连接目录中的 MiniMax Voice 可提交为 Provider Profile，但不得提前激活。
///
/// 数据构造（含关键数值的推导过程）：
///   active providers = 1（On-device）
///   MiniMax catalog Voices = 1（catalog_voice）
///   MiniMax draft tuple = speech-2.8-turbo + catalog_voice + Catalog
///   activation actions = 0（本测试只提交 Profile，不执行 Use This Voice 激活阶段）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 On-device 为 active，连接测试 MiniMax 凭据并加载一个目录 Voice。
///   2. 打开选择器，切到 MiniMax，将目录 Voice 写入候选 tuple。
///   3. 读取候选 readiness，再提交候选 Profile。
///   4. 读取已保存 MiniMax tuple 与 active Provider。
///
/// 预期结果：
///   - 正断言：目录匹配使候选达到 Ready，提交后 MiniMax Profile 保存精确 tuple。
///   - 正断言：active Provider 仍保持 On-device，等待独立激活动作。
///   - 负断言：提交 Profile 不得把 active Provider 偷换成 MiniMax，也不得保留旧 Voice。
#[test]
fn test_tts_picker_commits_ready_catalog_profile_without_implicit_activation() {
    let local_provider = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let local_model =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let local_voice = tts::TtsVoiceId::parse("af_bella").expect("本地 Voice ID 必须有效");
    let turbo = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let old_voice = tts::TtsVoiceId::parse("old_voice").expect("旧 MiniMax Voice ID 必须有效");
    let catalog_voice = tts::TtsVoiceId::parse("catalog_voice").expect("目录 Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(local_model),
            tts_voice_id: Some(local_voice),
            active_tts_provider_id: Some(local_provider.clone()),
            minimax_tts_model_id: Some(turbo.clone()),
            minimax_tts_voice_id: Some(old_voice.clone()),
            minimax_tts_voice_provenance: Some(tts::VoiceSelectionProvenance::Manual),
            minimax_tts_credential_ref: Some("picker-test-account".to_string()),
            minimax_tts_api_key: Some("synthetic-picker-key".to_string()),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("预置测试凭据必须允许连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::new(vec![tts::ProviderVoiceDescriptor::new(
                catalog_voice.clone(),
                "Catalog Voice",
                tts::TtsVoiceOrigin::System,
            )]),
        ),
        "当前 MiniMax 连接结果必须被接纳"
    );

    state.open_tts_picker();
    state.select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
    state.set_tts_picker_minimax_selection(
        turbo.clone(),
        catalog_voice.clone(),
        tts::VoiceSelectionProvenance::Catalog,
    );

    assert_eq!(
        state.tts_picker_minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "已连接目录中的候选 Voice 必须立即达到 Ready"
    );
    assert!(
        state.commit_tts_picker_minimax_profile(),
        "Ready 的 MiniMax 候选 Profile 必须可提交"
    );
    assert_eq!(
        state.values().minimax_tts_model_id(),
        Some(&turbo),
        "提交后 MiniMax Profile 必须保存 Turbo model"
    );
    assert_eq!(
        state.values().minimax_tts_voice_id(),
        Some(&catalog_voice),
        "提交后 MiniMax Profile 必须保存目录 Voice"
    );
    assert_eq!(
        state.values().active_tts_provider_id(),
        Some(&local_provider),
        "只提交候选 Profile 时 active Provider 必须继续保持 On-device"
    );
    assert_ne!(
        state
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "候选 Profile 提交不得隐式激活 MiniMax"
    );
    assert_ne!(
        state.values().minimax_tts_voice_id(),
        Some(&old_voice),
        "提交后 MiniMax Profile 不得继续保留旧 Voice"
    );
}

/// TTS-PICKER-3：本地模型管理中的 Select/Use 必须只更新候选 tuple，取消后恢复当前配置。
///
/// 数据构造（含关键数值的推导过程）：
///   active local tuple = Kokoro + af_bella
///   draft local tuple  = Matcha zh-en + default
///   activation actions = 0（未点击 Use This Voice）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 Kokoro/Bella 作为当前生效本地声音。
///   2. 打开选择器，把候选本地声音改为 Matcha/default。
///   3. 分别读取候选 tuple 和当前生效 tuple。
///   4. 取消选择器，确认临时候选被销毁。
///
/// 预期结果：
///   - 正断言：选择器必须保存 Matcha/default 候选。
///   - 正断言：取消前后当前配置都保持 Kokoro/Bella。
///   - 负断言：未 Apply 的 Matcha/default 不得泄漏到当前配置，取消后不得残留候选。
#[test]
fn test_local_tts_picker_selection_remains_draft_until_apply() {
    let local_provider = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let active_model =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let active_voice = tts::TtsVoiceId::parse("af_bella").expect("Bella Voice ID 必须有效");
    let draft_model =
        tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID).expect("Matcha model ID 必须有效");
    let draft_voice = tts::TtsVoiceId::parse("default").expect("Matcha Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(active_model.clone()),
            tts_voice_id: Some(active_voice.clone()),
            active_tts_provider_id: Some(local_provider),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );

    state.open_tts_picker();
    state.set_tts_picker_local_selection(draft_model.clone(), draft_voice.clone());

    assert_eq!(
        state.tts_picker_local_model_id(),
        Some(&draft_model),
        "本地选择器必须保存 Matcha 候选 model"
    );
    assert_eq!(
        state.tts_picker_local_voice_id(),
        Some(&draft_voice),
        "本地选择器必须保存 default 候选 Voice"
    );
    assert_eq!(
        state.values().tts_model_id(),
        Some(&active_model),
        "未 Apply 时当前 model 必须继续保持 Kokoro"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&active_voice),
        "未 Apply 时当前 Voice 必须继续保持 Bella"
    );
    assert_ne!(
        state.values().tts_model_id(),
        Some(&draft_model),
        "Matcha 候选不得提前泄漏到当前 model"
    );

    state.cancel_tts_picker();

    assert_eq!(
        state.tts_picker_local_model_id(),
        None,
        "取消后不得残留本地候选 model"
    );
    assert_eq!(
        state.values().tts_model_id(),
        Some(&active_model),
        "取消后当前 model 必须仍为 Kokoro"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&active_voice),
        "取消后当前 Voice 必须仍为 Bella"
    );
}
