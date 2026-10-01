use super::*;

/// TTS-PERSIST-1：旧版本地模型与音色必须迁移为 On-device，而全新配置不得暗选供应商。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy TTS fields = 2（model_id + voice_id）
///   explicit provider fields = 0（模拟升级前数据库）
///   fresh settings keys = 0（模拟首次启动）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离 SQLite 只写入旧版 Kokoro model 与 af_bella Voice。
///   2. 加载设置并读取 active provider、model 与 Voice。
///   3. 从另一个空 SQLite 加载全新设置并读取 active provider。
///
/// 预期结果：
///   - 正断言：旧配置激活 On-device，并完整保留 Kokoro 与 af_bella。
///   - 负断言：升级不得激活 MiniMax；全新安装不得暗选任意 TTS Provider。
#[test]
fn test_legacy_local_tts_migrates_without_defaulting_new_users() {
    let legacy_dir = tempfile::tempdir().expect("应创建旧版 TTS 迁移测试目录");
    let legacy_store = storage::SettingsStore::new(legacy_dir.path().join("legacy.sqlite"));
    legacy_store
        .set_string(
            super::values::KEY_TTS_MODEL_ID,
            tts::KOKORO_MULTI_LANG_MODEL_ID,
        )
        .expect("测试前置必须保存旧版 Kokoro model ID");
    legacy_store
        .set_string(super::values::KEY_TTS_VOICE_ID, "af_bella")
        .expect("测试前置必须保存旧版 af_bella Voice ID");

    let migrated = VoiceSettingsState::load_from_store(legacy_store);
    let fresh_dir = tempfile::tempdir().expect("应创建全新 TTS 设置测试目录");
    let fresh = VoiceSettingsState::load_from_store(storage::SettingsStore::new(
        fresh_dir.path().join("fresh.sqlite"),
    ));

    assert_eq!(
        migrated
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "旧版本地 TTS 选择必须迁移为 active On-device Provider"
    );
    assert_eq!(
        migrated
            .values()
            .tts_model_id()
            .map(tts::TtsModelId::as_str),
        Some(tts::KOKORO_MULTI_LANG_MODEL_ID),
        "迁移后必须精确保留旧版 Kokoro model ID"
    );
    assert_eq!(
        migrated
            .values()
            .tts_voice_id()
            .map(tts::TtsVoiceId::as_str),
        Some("af_bella"),
        "迁移后必须精确保留旧版 af_bella Voice ID"
    );
    assert_ne!(
        migrated
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "旧版本地配置不得在升级时被改成 MiniMax"
    );
    assert!(
        fresh.values().active_tts_provider_id().is_none(),
        "全新空设置必须等待用户显式选择，不能默认 On-device 或 MiniMax"
    );
}

/// TTS-PERSIST-2：本地与 MiniMax Profile 必须独立持久化，手动 Voice ID 不得被切换覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   providers = 2（On-device + MiniMax）
///   local tuple = Kokoro + af_bella
///   remote tuple = speech-2.8-turbo + external_voice_42 + Manual
///   provider switches = 2（MiniMax → On-device → MiniMax）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保存本地 tuple，再保存 MiniMax 的 model、手动 Voice ID 与 provenance。
///   2. 激活 MiniMax、切回 On-device、再次激活 MiniMax，并执行延迟写入。
///   3. 从相同 SQLite 重载并读取 active provider 与两个独立 Profile。
///
/// 预期结果：
///   - 正断言：重载后 active provider 为 MiniMax，手动 Voice ID 和 provenance 原样保留。
///   - 正断言：本地 Kokoro/af_bella 仍存在。
///   - 负断言：供应商切换不得把手动 Voice ID 替换为本地 Voice。
#[test]
fn test_provider_profiles_persist_manual_voice_independently() {
    let temp = tempfile::tempdir().expect("应创建 TTS Provider Profile 测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    let local_provider = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
        .expect("On-device Provider ID 必须有效");
    let minimax_provider = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("MiniMax Provider ID 必须有效");
    let local_model =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let local_voice = tts::TtsVoiceId::parse("af_bella").expect("本地 Voice ID 必须有效");
    let remote_model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let remote_voice = tts::TtsVoiceId::parse("external_voice_42").expect("手动 Voice ID 必须有效");

    state.select_tts_model_and_voice(Some(local_model.clone()), Some(local_voice.clone()));
    state.set_minimax_tts_selection(
        remote_model.clone(),
        remote_voice.clone(),
        tts::VoiceSelectionProvenance::Manual,
    );
    state.select_tts_provider(Some(minimax_provider.clone()));
    state.select_tts_provider(Some(local_provider));
    state.select_tts_provider(Some(minimax_provider.clone()));

    let writes = state.take_pending_writes();
    assert_eq!(
        writes.len(),
        6,
        "本地 2 个键、MiniMax 3 个键与最终 active Provider 应合并为 2 + 3 + 1 = 6 个持久化键"
    );
    for write in writes {
        write
            .execute()
            .expect("TTS Provider Profile 后台写入必须成功");
    }
    state.record_persistence_result(Ok(()));
    let reloaded = VoiceSettingsState::load_from_store(store);

    assert_eq!(
        reloaded.values().active_tts_provider_id(),
        Some(&minimax_provider),
        "两次切换后重载必须恢复最后显式激活的 MiniMax"
    );
    assert_eq!(
        reloaded.values().minimax_tts_model_id(),
        Some(&remote_model),
        "MiniMax Profile 必须保留 speech-2.8-turbo"
    );
    assert_eq!(
        reloaded.values().minimax_tts_voice_id(),
        Some(&remote_voice),
        "目录之外的手动 Voice ID 必须跨重启原样保留"
    );
    assert_eq!(
        reloaded.values().minimax_tts_voice_provenance(),
        tts::VoiceSelectionProvenance::Manual,
        "手动 Voice ID 的 provenance 必须跨重启保留为 Manual"
    );
    assert_eq!(
        reloaded.values().tts_model_id(),
        Some(&local_model),
        "切换到 MiniMax 不得删除本地 Kokoro Profile"
    );
    assert_eq!(
        reloaded.values().tts_voice_id(),
        Some(&local_voice),
        "切换到 MiniMax 不得删除本地 af_bella Voice"
    );
    assert_ne!(
        reloaded.values().minimax_tts_voice_id(),
        Some(&local_voice),
        "供应商切换不得用本地 Voice 覆盖 MiniMax 手动 Voice ID"
    );
}

/// TTS-PERSIST-3：手动 Voice ID 的成功预览证明必须绑定 credential、model 与 Voice 三元组。
///
/// 数据构造（含关键数值的推导过程）：
///   profile writes = 3（model + Voice ID + Manual provenance）
///   credential writes = 2（Keychain item + SQLite credential ref）
///   validation writes = 3（credential ref + model + Voice）
///   Total = 3 + 2 + 3 = 8 个合并后的后台写入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 配置手动 Voice，连接测试账户并记录一次成功 Preview。
///   2. 执行 8 个延迟写入，模拟重启后从 SQLite 和测试 Keychain 恢复。
///   3. 用同一账户重新连接，再把 model 从 Turbo 切换到 HD。
///
/// 预期结果：
///   - 正断言：同一 credential/Turbo/Voice 三元组重启后仍为 Ready。
///   - 正断言：改成 HD 后必须重新 Preview，状态为 VoiceValidationRequired。
///   - 负断言：旧 Turbo 的证明不得授权新的 HD 三元组。
#[test]
fn test_manual_voice_validation_persists_only_for_exact_tuple() {
    let temp = tempfile::tempdir().expect("应创建手动 Voice 校验持久化测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    let turbo = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let hd = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_HD_MODEL_ID)
        .expect("MiniMax HD model ID 必须有效");
    let voice = tts::TtsVoiceId::parse("external_voice_42").expect("测试手动 Voice ID 必须有效");
    state.set_minimax_tts_selection(turbo.clone(), voice, tts::VoiceSelectionProvenance::Manual);
    state.update_minimax_api_key_draft("test-minimax-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("测试 API Key 应允许启动 MiniMax 连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "当前 generation 的成功连接必须被接纳"
    );
    let profile_generation = state.minimax_profile_generation();
    assert!(
        state.mark_minimax_manual_voice_validated(profile_generation),
        "当前 generation 的手动 Voice 预览必须生成验证证明"
    );

    let writes = state.take_pending_writes();
    assert_eq!(
        writes.len(),
        8,
        "应精确写入 3 个 Profile 键、2 个凭据操作和 3 个校验证明键"
    );
    for write in writes {
        write.execute().expect("测试 TTS 设置后台写入必须成功");
    }
    state.record_persistence_result(Ok(()));

    let mut reloaded = VoiceSettingsState::load_from_store(store.clone());
    let account = reloaded
        .minimax_credential_ref_to_load()
        .expect("重启后必须从 SQLite 恢复非敏感 credential ref");
    let secret = crate::settings::voice::credentials::load(&store, &account)
        .expect("测试 SQLite 读取必须成功");
    assert!(
        reloaded.apply_loaded_minimax_credential(&account, Ok(secret)),
        "同一 credential ref 必须恢复对应测试 Keychain secret"
    );
    let (generation, key) = reloaded
        .begin_minimax_connection()
        .expect("恢复凭据后必须能重新检查账户");
    assert!(
        reloaded.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "重启后的当前连接结果必须被接纳"
    );
    assert_eq!(
        reloaded.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "完全相同的 credential/Turbo/Voice 三元组必须恢复 Ready"
    );

    reloaded.set_minimax_tts_model(hd);
    assert_eq!(
        reloaded.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "model 变化后手动 Voice 必须重新 Preview 校验"
    );
    assert_ne!(
        reloaded.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "旧 Turbo 校验证明不得错误授权 HD"
    );
}

/// TTS-CRED-1：替换 API Key 的连接校验失败时必须保留上一份可用 Keychain 凭据。
///
/// 数据构造（含关键数值的推导过程）：
///   credentials = 2（working candidate + rejected replacement）
///   accepted connections = 1；replacement attempts = 2；rejected connections = 1
///   replacement persistence writes = 0（失败候选不得落盘）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 成功连接 working credential 并完成其延迟写入。
///   2. 输入 replacement candidate，连续发起两次连接，再让最新请求返回 Authentication 错误。
///   3. 再次读取已保存 credential ref、内存 secret 与待写入队列。
///
/// 预期结果：
///   - 正断言：失败被识别为 rejected candidate，working account 继续保持 Ready。
///   - 正断言：旧 ref 与旧 secret 均保留。
///   - 正断言：失败候选仍留在草稿，可供用户修正后重试。
///   - 负断言：失败候选不得产生任何 Keychain 或 SQLite 写入。
#[test]
fn test_failed_minimax_key_replacement_preserves_working_credential() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 凭据原子替换测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    state.update_minimax_api_key_draft("working-test-key".to_string());
    let (generation, working_key) = state
        .begin_minimax_connection()
        .expect("working candidate 应启动连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            working_key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "working candidate 的当前连接结果必须被接纳"
    );
    let writes = state.take_pending_writes();
    for write in writes {
        write.execute().expect("working credential 必须成功持久化");
    }
    state.record_persistence_result(Ok(()));
    let working_ref = state
        .values()
        .minimax_tts_credential_ref()
        .expect("成功连接后必须存在 credential ref")
        .to_string();

    state.update_minimax_api_key_draft("rejected-test-key".to_string());
    let (first_replacement_generation, rejected_key) = state
        .begin_minimax_connection()
        .expect("replacement candidate 应允许后台校验");
    assert_eq!(
        rejected_key, "rejected-test-key",
        "连接请求必须使用新草稿，而不是静默复用旧 secret"
    );
    let (generation, repeated_key) = state
        .begin_minimax_connection()
        .expect("Checking 期间重复点击必须启动新的当前请求");
    assert_eq!(
        repeated_key, "rejected-test-key",
        "重复连接仍必须使用 replacement 草稿"
    );
    assert_ne!(
        generation, first_replacement_generation,
        "重复连接必须推进 generation，使前一请求失效"
    );
    assert_eq!(
        state.record_minimax_connection_failure(
            generation,
            tts::TtsErrorKind::Authentication,
            "MiniMax credential was rejected",
        ),
        MiniMaxConnectionFailureDisposition::CandidateRejected,
        "当前 replacement 失败必须只拒绝候选，不得标记 working credential 失效"
    );

    assert_eq!(
        state.minimax_account_status(),
        tts::RemoteAccountStatus::Ready,
        "replacement 认证失败后 working account 必须继续保持 Ready"
    );
    assert_eq!(
        state.values().minimax_tts_credential_ref(),
        Some(working_ref.as_str()),
        "失败替换必须保留上一份 working credential ref"
    );
    assert_eq!(
        state.values().minimax_tts_api_key(),
        Some("working-test-key"),
        "失败替换必须保留上一份 working secret"
    );
    let (_, retry_key) = state
        .begin_minimax_connection()
        .expect("失败候选应留在草稿供用户修正或重试");
    assert_eq!(
        retry_key, "rejected-test-key",
        "重试应继续使用未持久化的 replacement 草稿"
    );
    assert!(
        state.take_pending_writes().is_empty(),
        "认证失败候选不得写入 Keychain 或 SQLite"
    );
}

/// TTS-SEARCH-1：Voice 搜索只过滤当前页面，不得持久化或改写已选 Voice。
///
/// 数据构造（含关键数值的推导过程）：
///   search edits = 2（"narrator" → 空字符串）
///   persistent keys = 0（搜索属于瞬时 UI 状态）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 开始编辑 MiniMax Voice 搜索并自动保存 narrator 查询。
///   2. 读取查询值，再自动保存空字符串以清除过滤。
///   3. 检查延迟持久化队列。
///
/// 预期结果：
///   - 正断言：查询与清空操作都立即反映到状态。
///   - 负断言：两次搜索编辑均不得生成 SQLite 写入。
#[test]
fn test_minimax_voice_search_is_ephemeral_and_clearable() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.begin_edit(VoiceSettingsField::MiniMaxVoiceSearch);
    let saved = state.auto_save_edit_value(
        VoiceSettingsField::MiniMaxVoiceSearch,
        "narrator".to_string(),
    );
    assert_eq!(
        saved.as_deref(),
        Some("narrator"),
        "Voice 搜索应接受非空查询"
    );
    assert_eq!(
        state.minimax_voice_search(),
        "narrator",
        "搜索状态必须立即反映 narrator 查询"
    );

    let cleared = state.auto_save_edit_value(VoiceSettingsField::MiniMaxVoiceSearch, String::new());
    assert_eq!(
        cleared.as_deref(),
        Some(""),
        "Voice 搜索必须允许空字符串清除过滤"
    );
    assert_eq!(state.minimax_voice_search(), "", "清除后搜索状态必须为空");
    assert!(
        state.take_pending_writes().is_empty(),
        "搜索属于瞬时 UI 状态，不得写入 SQLite"
    );
}

/// TTS-CRED-2：用户清除 MiniMax 凭据后必须删除引用并立即失去 Ready 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   initial credential writes = Keychain(1) + SQLite ref(1) = 2
///   clear writes = Keychain delete(1) + SQLite ref delete(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连接一个测试凭据并执行最初两项持久化。
///   2. 调用公开的 clear_minimax_credential。
///   3. 执行删除写入并重新读取 credential ref 与 readiness。
///
/// 预期结果：
///   - 正断言：恰好生成并成功执行两项删除，状态回到 Unconfigured。
///   - 负断言：内存与设置值均不再保留 API Key 或 credential ref。
#[test]
fn test_clear_minimax_credential_removes_secret_reference_and_readiness() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 凭据清除测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    state.update_minimax_api_key_draft("clear-test-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("测试凭据应能开始连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "当前测试连接结果必须被接纳"
    );
    let initial_writes = state.take_pending_writes();
    assert_eq!(
        initial_writes.len(),
        2,
        "初次保存应包含 Keychain 与 SQLite 两项写入"
    );
    for write in initial_writes {
        write.execute().expect("初次测试凭据必须成功保存");
    }
    state.record_persistence_result(Ok(()));

    state.clear_minimax_credential();
    let clear_writes = state.take_pending_writes();

    assert_eq!(
        clear_writes.len(),
        2,
        "清除必须包含 Keychain item 与 SQLite ref 两项删除"
    );
    for write in clear_writes {
        write.execute().expect("测试凭据删除必须成功");
    }
    state.record_persistence_result(Ok(()));
    assert!(
        state.values().minimax_tts_api_key().is_none(),
        "清除后内存不得保留 MiniMax API Key"
    );
    assert!(
        state.values().minimax_tts_credential_ref().is_none(),
        "清除后 SQLite 值不得保留 credential ref"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Unconfigured,
        "缺少凭据后 MiniMax 必须回到 Unconfigured"
    );
    assert_ne!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "已清除凭据不得继续显示 Ready"
    );
}
