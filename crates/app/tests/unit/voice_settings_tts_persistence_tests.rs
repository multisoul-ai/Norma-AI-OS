use super::*;

/// TTS-PERSIST-4：替换凭据落盘失败后必须完整恢复旧 working credential 与手动 Voice 证明。
///
/// 数据构造（含关键数值的推导过程）：
///   initial batch = profile(3) + credential(2) + validation proof(3) = 8 writes
///   replacement batch = validation deletes(3) + credential store/ref(2) = 5 writes
///   compensation batch = credential ref restore(1) + rejected item delete(1) + proof restore(3)
///                      = 5 writes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保存并验证旧 Key + Turbo + manual_voice_restore，完成 8 项初始写入。
///   2. 验证新 Key，取走包含 5 项操作的替换批次，再模拟该批次失败。
///   3. 读取回滚后的账户、三元组 readiness 与 5 项补偿写入。
///
/// 预期结果：
///   - 正断言：旧 ref、secret、Ready 状态和手动 Voice 证明全部恢复。
///   - 正断言：失败后生成精确的 ref/Keychain/proof 补偿批次。
///   - 负断言：新 Key 落盘失败不得把旧 working credential 标成 InvalidCredential。
#[test]
fn test_failed_minimax_replacement_persistence_restores_working_validation() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 替换落盘失败测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    let model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let voice = tts::TtsVoiceId::parse("manual_voice_restore").expect("测试手动 Voice ID 必须有效");
    state.set_minimax_tts_selection(model, voice, tts::VoiceSelectionProvenance::Manual);
    state.update_minimax_api_key_draft("working-persistence-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("旧 working Key 必须能开始验证");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "旧 working Key 的当前连接结果必须被接纳"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "旧 working 三元组必须成功记录手动 Voice 证明"
    );
    let initial_batch = state.take_pending_writes();
    assert_eq!(
        initial_batch.len(),
        8,
        "初始批次必须是 profile(3) + credential(2) + proof(3) = 8"
    );
    for write in initial_batch {
        write.execute().expect("旧 working 配置必须成功持久化");
    }
    state.record_persistence_result(Ok(()));
    let working_ref = state
        .values()
        .minimax_tts_credential_ref()
        .expect("旧 working 配置必须存在 credential ref")
        .to_string();
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "替换前旧 working 三元组必须为 Ready"
    );

    state.update_minimax_api_key_draft("replacement-persistence-key".to_string());
    let (replacement_generation, replacement_key) = state
        .begin_minimax_connection()
        .expect("新 replacement Key 必须能开始验证");
    assert!(
        state.record_minimax_connection_success(
            replacement_generation,
            replacement_key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "新 replacement Key 的连接成功结果必须先进入待持久化状态"
    );
    let replacement_profile_generation = state.minimax_profile_generation();
    assert!(
        state.begin_minimax_connection().is_err(),
        "replacement credential 尚未落盘时不得启动第二次连接并覆盖 rollback"
    );
    let replacement_batch = state.take_pending_writes();
    assert_eq!(
        replacement_batch.len(),
        5,
        "替换批次必须是 validation deletes(3) + credential writes(2) = 5"
    );

    state.record_persistence_result(Err("simulated replacement persistence failure".to_string()));

    assert_eq!(
        state.values().minimax_tts_credential_ref(),
        Some(working_ref.as_str()),
        "替换落盘失败必须恢复旧 working credential ref"
    );
    assert_eq!(
        state.values().minimax_tts_api_key(),
        Some("working-persistence-key"),
        "替换落盘失败必须恢复旧 working secret"
    );
    assert_eq!(
        state.minimax_account_status(),
        tts::RemoteAccountStatus::Ready,
        "替换落盘失败不得把旧 working account 标成 InvalidCredential"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "恢复旧凭据与证明后，旧手动 Voice 三元组必须继续 Ready"
    );
    let compensation_batch = state.take_pending_writes();
    assert_eq!(
        compensation_batch.len(),
        5,
        "补偿批次必须是 ref restore(1) + rejected delete(1) + proof restore(3) = 5"
    );
    assert_ne!(
        state.minimax_account_status(),
        tts::RemoteAccountStatus::InvalidCredential,
        "新 Key 保存失败不代表旧 working credential 无效"
    );
    assert_ne!(
        state.minimax_profile_generation(),
        replacement_profile_generation,
        "凭据回滚必须推进 Profile generation，使新 Key 启动的 Preview 失效"
    );
}

/// TTS-PERSIST-5：Preview 期间修改 MiniMax Profile 后，旧响应不得验证新三元组。
///
/// 数据构造（含关键数值的推导过程）：
///   preview profile = Turbo + manual_voice_a
///   edited profile = Turbo + manual_voice_b
///   profile generations = G 与 G + 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 配置并连接 manual_voice_a，记录 Preview 启动时的 generation G。
///   2. Preview 返回前把 Voice 改为 manual_voice_b，使 generation 推进为 G + 1。
///   3. 用旧 G 尝试记录证明，再用当前 generation 记录证明。
///
/// 预期结果：
///   - 正断言：当前 generation 能为实际当前三元组生成证明并进入 Ready。
///   - 负断言：旧 G 响应不得为未被该响应合成的 manual_voice_b 生成证明。
#[test]
fn test_stale_minimax_preview_cannot_validate_edited_profile() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let voice_a = tts::TtsVoiceId::parse("manual_voice_a").expect("Voice A 必须有效");
    let voice_b = tts::TtsVoiceId::parse("manual_voice_b").expect("Voice B 必须有效");
    state.set_minimax_tts_selection(
        model.clone(),
        voice_a,
        tts::VoiceSelectionProvenance::Manual,
    );
    state.update_minimax_api_key_draft("preview-generation-key".to_string());
    let (connection_generation, key) = state
        .begin_minimax_connection()
        .expect("测试 Key 必须能开始连接");
    assert!(
        state.record_minimax_connection_success(
            connection_generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "测试连接结果必须被当前 generation 接纳"
    );
    let preview_profile_generation = state.minimax_profile_generation();

    state.set_minimax_tts_selection(model, voice_b, tts::VoiceSelectionProvenance::Manual);

    assert!(
        !state.mark_minimax_manual_voice_validated(preview_profile_generation),
        "Voice A 的旧 Preview 响应不得验证后来编辑的 Voice B"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "旧 Preview 被拒绝后 Voice B 必须仍要求单独验证"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "匹配当前 Profile generation 的 Preview 才能验证 Voice B"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "Voice B 只有在匹配当前 generation 的证明后才能 Ready"
    );
}

/// TTS-PERSIST-6：手动 Voice 证明落盘失败后必须撤销内存 Ready 并补偿删除残留键。
///
/// 数据构造（含关键数值的推导过程）：
///   initial batch = profile(3) + credential(2) = 5 writes
///   validation batch = credential/model/Voice proof = 3 writes
///   compensation batch = credential/model/Voice deletes = 3 writes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保存 Key + Turbo + manual_voice_proof_failure，但暂不记录 Preview 证明。
///   2. 记录成功 Preview，使内存暂时 Ready，并取走 3 项证明写入。
///   3. 模拟证明批次失败，读取 readiness 与 3 项补偿删除。
///
/// 预期结果：
///   - 正断言：失败后回到 VoiceValidationRequired，并生成三项补偿删除。
///   - 负断言：未持久化的证明不得继续让当前会话保持 Ready。
#[test]
fn test_failed_minimax_validation_persistence_revokes_in_memory_readiness() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 证明落盘失败测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    let model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let voice =
        tts::TtsVoiceId::parse("manual_voice_proof_failure").expect("测试手动 Voice ID 必须有效");
    state.set_minimax_tts_selection(model, voice, tts::VoiceSelectionProvenance::Manual);
    state.update_minimax_api_key_draft("proof-persistence-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("测试 Key 必须能开始验证");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "测试 Key 的当前连接结果必须被接纳"
    );
    let initial_batch = state.take_pending_writes();
    assert_eq!(
        initial_batch.len(),
        5,
        "初始批次必须是 profile(3) + credential(2) = 5"
    );
    for write in initial_batch {
        write
            .execute()
            .expect("初始 MiniMax Profile 必须成功持久化");
    }
    state.record_persistence_result(Ok(()));
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "手动 Voice 在 Preview 证明前必须要求验证"
    );

    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "匹配当前 Profile 的成功 Preview 必须暂时记录证明"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "证明写入完成前内存会暂时反映本次成功 Preview"
    );
    let validation_batch = state.take_pending_writes();
    assert_eq!(
        validation_batch.len(),
        3,
        "证明批次必须精确包含 credential/model/Voice 三个键"
    );

    state.record_persistence_result(Err("simulated proof persistence failure".to_string()));

    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "证明落盘失败后必须撤销内存 Ready，禁止激活未持久化三元组"
    );
    assert_ne!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "失败证明不得继续授权当前会话"
    );
    let compensation_batch = state.take_pending_writes();
    assert_eq!(
        compensation_batch.len(),
        3,
        "失败后必须生成 credential/model/Voice 三项补偿删除"
    );
}

/// TTS-PERSIST-7：旧证明删除批次不得提前消费随后排队的新证明 rollback。
///
/// 数据构造（含关键数值的推导过程）：
///   initial validated tuple = Turbo + manual_voice_batch_a
///   old-proof removal batch = model update(1) + proof deletes(3) = 4 writes
///   new-proof batch = credential/model/Voice proof sets = 3 writes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完成初始凭据与 Voice A 证明落盘，使状态为 Ready。
///   2. 切换 HD，取走含旧证明删除的 4 项批次；该批次运行期间记录新 Preview 证明。
///   3. 完成旧删除批次，确认新 rollback 仍在；再让新证明批次失败。
///
/// 预期结果：
///   - 正断言：旧删除批次完成后，新证明 rollback 仍阻止下一次 Preview。
///   - 正断言：新证明批次失败会撤销 Ready。
///   - 负断言：旧批次不得冒充新证明写入成功并提前消费 rollback。
#[test]
fn test_old_validation_delete_batch_cannot_finish_new_validation_write() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 证明批次归属测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    let turbo = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let hd = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_HD_MODEL_ID)
        .expect("MiniMax HD model ID 必须有效");
    let voice = tts::TtsVoiceId::parse("manual_voice_batch_a").expect("测试手动 Voice ID 必须有效");
    state.set_minimax_tts_selection(turbo, voice, tts::VoiceSelectionProvenance::Manual);
    state.update_minimax_api_key_draft("validation-batch-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("测试 Key 必须能开始验证");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "初始连接结果必须被当前 generation 接纳"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "初始 Voice A 证明必须成功记录"
    );
    let initial_batch = state.take_pending_writes();
    assert_eq!(
        initial_batch.len(),
        8,
        "初始批次必须是 profile(3) + credential(2) + proof(3) = 8"
    );
    for write in initial_batch {
        write.execute().expect("初始 Voice A 配置必须成功持久化");
    }
    state.record_persistence_result(Ok(()));
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "测试前置 Voice A 三元组必须为 Ready"
    );

    state.set_minimax_tts_model(hd);
    let old_delete_batch = state.take_pending_writes();
    assert_eq!(
        old_delete_batch.len(),
        4,
        "切换 model 必须排入 model update(1) + old proof deletes(3) = 4"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "旧删除批次运行时，新 Profile 的成功 Preview 应排入下一批证明写入"
    );

    state.record_persistence_result(Ok(()));

    assert!(
        state.minimax_persistence_blocks_preview(),
        "旧删除批次完成不得消费后来创建的新证明 rollback"
    );
    let new_validation_batch = state.take_pending_writes();
    assert_eq!(
        new_validation_batch.len(),
        3,
        "下一批必须精确包含新 credential/model/Voice 三项证明写入"
    );
    state.record_persistence_result(Err("simulated new proof failure".to_string()));
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "新证明批次失败必须撤销 Ready，证明 rollback 未被旧批次偷取消费"
    );
    assert_ne!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "旧删除批次不得冒充新证明保存成功"
    );
}

/// TTS-PERSIST-8：重复选择完全相同的 MiniMax Profile 不得清除已有手动 Voice 证明。
///
/// 数据构造（含关键数值的推导过程）：
///   profile tuple = Turbo + manual_voice_unchanged + Manual
///   repeated model selections = 1
///   repeated full tuple selections = 1
///   expected generation delta = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 配置、连接并验证手动 Voice，使当前三元组进入 Ready。
///   2. 重复设置同一个 Turbo model，再重复提交完全相同的三元组。
///   3. 读取两个 changed 返回值、Profile generation 与 readiness。
///
/// 预期结果：
///   - 正断言：重复操作均报告 unchanged，generation 保持不变且仍为 Ready。
///   - 负断言：原样提交不得清除证明并退回 VoiceValidationRequired。
#[test]
fn test_unchanged_minimax_profile_preserves_manual_validation() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let voice =
        tts::TtsVoiceId::parse("manual_voice_unchanged").expect("测试手动 Voice ID 必须有效");
    assert!(
        state.set_minimax_tts_selection(
            model.clone(),
            voice.clone(),
            tts::VoiceSelectionProvenance::Manual,
        ),
        "首次设置三元组必须报告 changed"
    );
    state.update_minimax_api_key_draft("unchanged-profile-key".to_string());
    let (connection_generation, key) = state
        .begin_minimax_connection()
        .expect("测试 Key 必须能开始连接");
    assert!(
        state.record_minimax_connection_success(
            connection_generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "测试连接结果必须被当前 generation 接纳"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "测试三元组必须成功记录手动 Voice 证明"
    );
    let profile_generation = state.minimax_profile_generation();

    assert!(
        !state.set_minimax_tts_model(model.clone()),
        "重复点击已选 Turbo model 必须报告 unchanged"
    );
    assert!(
        !state.set_minimax_tts_selection(model, voice, tts::VoiceSelectionProvenance::Manual,),
        "原样提交完全相同的三元组必须报告 unchanged"
    );
    assert_eq!(
        state.minimax_profile_generation(),
        profile_generation,
        "相同 Profile 不得推进 generation"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "相同 Profile 操作必须保留已有手动 Voice 证明和 Ready"
    );
    assert_ne!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "相同 Profile 不得迫使用户重新 Preview"
    );
}
