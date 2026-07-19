use super::*;

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
