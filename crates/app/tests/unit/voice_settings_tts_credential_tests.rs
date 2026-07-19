use super::*;

/// TTS-CRED-4：用户提交 API Key 草稿后，完整 MiniMax Profile 必须显示 Configured 而非 Unconfigured。
///
/// 数据构造（含关键数值的推导过程）：
///   credential drafts = 1（draft-key-for-readiness）
///   profile fields = 2（speech-2.8-turbo + manual_voice_readiness）
///   remote checks = 0（提交本地草稿不应同步访问网络）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立没有持久化凭据的全新 Voice Settings 状态。
///   2. 先选择 model 与手动 Voice ID，确认没有凭据时仍为 Unconfigured。
///   3. 提交一个非空 MiniMax API Key，再读取 readiness、输入框显示与 credential ref。
///
/// 预期结果：
///   - 正断言：非空草稿与完整 Profile 应进入 Configured，提示用户继续验证。
///   - 正断言：输入框失焦后仍显示掩码，不得把有效草稿显示成 Not configured。
///   - 负断言：尚未验证的草稿不得错误进入 Ready，也不得伪造持久化 credential ref。
#[test]
fn test_minimax_api_key_draft_is_configured_before_remote_validation() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let voice =
        tts::TtsVoiceId::parse("manual_voice_readiness").expect("测试手动 Voice ID 必须有效");

    state.set_minimax_tts_selection(model, voice, tts::VoiceSelectionProvenance::Manual);
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Unconfigured,
        "只有 model 与 Voice、没有 API Key 时必须保持 Unconfigured"
    );

    let saved = state.commit_edit_value(
        VoiceSettingsField::MiniMaxApiKey,
        "draft-key-for-readiness".to_string(),
    );

    assert_eq!(
        saved.as_deref(),
        Some("draft-key-for-readiness"),
        "提交非空 MiniMax API Key 必须保留为待验证草稿"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Configured,
        "API Key 草稿与完整 Profile 必须显示 Configured，而不是 Not Configured"
    );
    assert_ne!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "未经远端验证的 API Key 草稿不得直接进入 Ready"
    );
    assert_eq!(
        state.row_value(VoiceSettingsField::MiniMaxApiKey),
        "*********",
        "已提交的 API Key 草稿在输入框失焦后必须继续显示配置掩码"
    );
    assert_ne!(
        state.row_value(VoiceSettingsField::MiniMaxApiKey),
        "Not configured",
        "有效 API Key 草稿不得在保存后立刻显示 Not configured"
    );
    assert!(
        state.values().minimax_tts_credential_ref().is_none(),
        "远端验证成功前不得伪造已持久化 credential ref"
    );
}

/// TTS-CRED-3：清除凭据的持久化失败必须保留内存 secret 并补偿恢复 SQLite ref。
///
/// 数据构造（含关键数值的推导过程）：
///   initial writes = Keychain store(1) + SQLite ref set(1) = 2
///   clear batch = SQLite ref delete(1) + Keychain delete(1) = 2
///   compensation writes = SQLite ref restore(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 成功保存 working credential，记录其 opaque ref 与内存 secret。
///   2. 开始清除并取走两项后台写入，模拟该批次在完成前失败。
///   3. 记录失败结果，再读取状态与补偿写入。
///
/// 预期结果：
///   - 正断言：原 ref/secret 均保留，并生成一项 ref 恢复写入。
///   - 负断言：失败清除不得让 UI 进入无凭据状态，也不得保持 pending 标记。
#[test]
fn test_failed_minimax_credential_clear_restores_reference_and_keeps_secret() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 清除失败测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    state.update_minimax_api_key_draft("working-clear-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("working credential 应能开始连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "working credential 的当前连接结果必须被接纳"
    );
    let initial_writes = state.take_pending_writes();
    assert_eq!(initial_writes.len(), 2, "初次保存必须包含两项原子前置写入");
    for write in initial_writes {
        write.execute().expect("working credential 必须成功保存");
    }
    state.record_persistence_result(Ok(()));
    let working_ref = state
        .values()
        .minimax_tts_credential_ref()
        .expect("成功保存后必须存在 opaque ref")
        .to_string();

    state.clear_minimax_credential();
    let clear_batch = state.take_pending_writes();
    assert_eq!(
        clear_batch.len(),
        2,
        "清除批次必须包含 ref 与 Keychain 两项删除"
    );
    assert!(
        state.begin_minimax_connection().is_err(),
        "凭据清除尚未落盘时不得启动新连接并与 clear rollback 串批次"
    );
    state.record_persistence_result(Err("simulated clear failure".to_string()));

    assert_eq!(
        state.values().minimax_tts_credential_ref(),
        Some(working_ref.as_str()),
        "清除失败后必须继续引用 working credential"
    );
    assert_eq!(
        state.values().minimax_tts_api_key(),
        Some("working-clear-key"),
        "清除失败后内存必须保留 working secret"
    );
    assert!(
        !state.minimax_credential_clear_pending(),
        "失败完成后不得遗留 clear pending 标记"
    );
    let compensation = state.take_pending_writes();
    assert_eq!(
        compensation.len(),
        1,
        "失败后必须生成一项 SQLite ref 补偿恢复写入"
    );
    assert!(
        state.values().minimax_tts_credential_ref().is_some(),
        "失败清除不得让 UI 显示无凭据"
    );
}

/// TTS-CRED-5：旧普通设置批次完成时不得提前提交随后排队的 MiniMax Keychain 写入。
///
/// 数据构造（含关键数值的推导过程）：
///   batch 1 = input device 设置(1)
///   batch 2 = Keychain store(1) + SQLite credential ref(1) = 2
///   persistence completions = 2（先普通设置，后 MiniMax credential）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动只有 input device 的第一批写入，使 persistence_running = true。
///   2. 第一批仍运行时完成 MiniMax 目录验证，把新凭据两项写入排到下一批。
///   3. 先完成第一批，检查 credential rollback；再执行并完成第二批。
///
/// 预期结果：
///   - 正断言：第一批结束后 MiniMax credential 仍 pending，第二批精确包含两项写入。
///   - 正断言：第二批成功后 credential pending 才结束。
///   - 负断言：普通设置成功不得冒充 Keychain 凭据保存成功。
#[test]
fn test_unrelated_persistence_batch_cannot_finish_queued_minimax_credential() {
    let temp = tempfile::tempdir().expect("应创建 MiniMax 批次归属测试目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store);
    state.select_input_device(Some("device-before-credential".to_string()));
    let first_batch = state.take_pending_writes();
    assert_eq!(first_batch.len(), 1, "第一批必须只有 input device 设置写入");

    state.update_minimax_api_key_draft("batch-owned-key".to_string());
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("MiniMax candidate 应能在旧批次运行时验证");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "当前 MiniMax 连接结果必须被接纳并排队凭据写入"
    );
    assert!(
        state.credential_persistence_pending(),
        "目录验证成功后必须存在尚未完成的 credential rollback"
    );

    for write in first_batch {
        write.execute().expect("第一批普通设置写入必须成功");
    }
    let first_completion = state.record_persistence_result(Ok(()));
    assert!(
        !first_completion.minimax_credential,
        "第一批不含 MiniMax Keychain 写入，不得报告 credential 完成"
    );
    assert!(
        state.credential_persistence_pending(),
        "第一批完成后 MiniMax credential rollback 必须继续等待第二批"
    );

    let second_batch = state.take_pending_writes();
    assert_eq!(
        second_batch.len(),
        2,
        "第二批必须精确包含 Keychain store 与 SQLite credential ref"
    );
    for write in second_batch {
        write
            .execute()
            .expect("第二批 MiniMax credential 写入必须成功");
    }
    let second_completion = state.record_persistence_result(Ok(()));
    assert!(
        second_completion.minimax_credential,
        "只有第二批才应报告 MiniMax credential 已完成"
    );
    assert!(
        !state.credential_persistence_pending(),
        "第二批成功后不得遗留 credential rollback"
    );
}

/// TTS-CRED-6：验证 Key A 期间改成 Key B 后，A 的过期成功结果不得覆盖 B。
///
/// 数据构造（含关键数值的推导过程）：
///   credential drafts = 2（candidate-a → candidate-b）
///   remote results = 1（candidate-a 的迟到成功）
///   expected credential writes = 0（过期结果必须丢弃）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入 Key A 并开始连接，记录其 generation。
///   2. 连接返回前输入 Key B，使 A 的 generation 失效。
///   3. 注入 A 的成功结果，再开始下一次连接读取实际候选。
///
/// 预期结果：
///   - 正断言：A 结果被判定 stale，下一次连接使用 B。
///   - 负断言：A 不得写入 Keychain、生成 credential ref 或清空 B 草稿。
#[test]
fn test_editing_minimax_key_invalidates_inflight_candidate_result() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.update_minimax_api_key_draft("candidate-a".to_string());
    let (generation_a, key_a) = state
        .begin_minimax_connection()
        .expect("Key A 应能开始连接");
    assert_eq!(key_a, "candidate-a", "第一次连接必须使用 Key A");
    state.update_minimax_api_key_draft("candidate-b".to_string());

    assert!(
        !state.record_minimax_connection_success(
            generation_a,
            key_a,
            tts::ProviderVoiceCatalog::default(),
        ),
        "编辑 Key B 后，Key A 的迟到成功结果必须被判定 stale"
    );
    assert!(
        state.take_pending_writes().is_empty(),
        "过期 Key A 不得生成 Keychain 或 SQLite 写入"
    );
    assert!(
        state.values().minimax_tts_credential_ref().is_none(),
        "过期 Key A 不得生成 credential ref"
    );
    let (_, key_b) = state
        .begin_minimax_connection()
        .expect("编辑后的 Key B 必须仍可验证");
    assert_eq!(key_b, "candidate-b", "下一次连接必须使用最新 Key B");
}

/// TTS-CRED-7：尚未验证的 API Key 草稿必须可通过 Clear 立即移除。
///
/// 数据构造（含关键数值的推导过程）：
///   credential drafts = 1（clearable-draft）
///   persisted credential refs = 0
///   expected background writes = 0（草稿只存在内存）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入一个尚未连接的 MiniMax API Key 草稿。
///   2. 开始验证后调用与 UI Clear Credential 相同的状态入口。
///   3. 注入清除前请求的迟到成功，再读取配置状态、连接入口与待写队列。
///
/// 预期结果：
///   - 正断言：清除后配置回到无凭据状态，连接明确要求重新输入 Key。
///   - 负断言：纯内存草稿清除不得伪造 Keychain/SQLite 删除写入。
#[test]
fn test_clear_minimax_credential_removes_unvalidated_draft_without_writes() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.update_minimax_api_key_draft("clearable-draft".to_string());
    assert!(
        state.minimax_credential_configured(),
        "测试前置草稿必须被 UI 视为已输入"
    );
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("清除前草稿应能开始验证");

    state.clear_minimax_credential();

    assert!(
        !state.minimax_credential_configured(),
        "Clear 后不得继续把已删除草稿显示为 configured"
    );
    assert!(
        !state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "Clear 后清除前请求的迟到成功必须被判定 stale"
    );
    assert!(
        state.begin_minimax_connection().is_err(),
        "Clear 后连接入口必须要求重新输入 API Key"
    );
    assert!(
        state.take_pending_writes().is_empty(),
        "清除纯内存草稿不得生成 Keychain 或 SQLite 写入"
    );
}

/// TTS-CRED-8：MiniMax Preview 的进行中状态只能由同一 generation 的完成事件清除。
///
/// 数据构造（含关键数值的推导过程）：
///   active preview generation = 7（任意非零当前请求编号）
///   stale completion generation = 6（7 - 1，代表更早请求）
///   matching completion generation = 7
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 generation 7 开始 Preview，读取进行中标记与状态文案。
///   2. 注入 generation 6 的过期完成事件，确认当前状态未被清除。
///   3. 注入 generation 7 的匹配完成事件，确认进行中标记结束。
///
/// 预期结果：
///   - 正断言：开始后立即进入进行中状态并提供可见反馈。
///   - 正断言：匹配完成事件能够结束当前 Preview 状态。
///   - 负断言：过期完成事件不得清除较新的 Preview 状态。
#[test]
fn test_minimax_preview_feedback_is_generation_aware() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    state.begin_minimax_preview(7);

    assert!(
        state.minimax_preview_in_progress(),
        "开始 MiniMax Preview 后必须立即显示进行中状态"
    );
    assert_eq!(
        state.status.as_ref().map(|status| status.message.as_str()),
        Some("Generating MiniMax preview…"),
        "开始 Preview 后必须提供可见的生成中反馈"
    );

    state.finish_minimax_preview(6);
    assert!(
        state.minimax_preview_in_progress(),
        "旧 generation 的完成事件不得清除当前 Preview 状态"
    );

    state.finish_minimax_preview(7);
    assert!(
        !state.minimax_preview_in_progress(),
        "匹配 generation 的完成事件必须结束当前 Preview 状态"
    );
}
