use super::*;

/// VS-11：Settings 持久化必须单飞串行，后续修改不能与首批写入并行。
///
/// 数据构造：先选择 OpenAI 并取走首批写入，再在首批未完成时切回 Ollama。
/// 执行过程：第二次尝试 drain 应为空；记录首批完成后再次 drain 并执行。
/// 预期结果：两批各一个写入且严格串行；数据库最终值为最后选择的 Ollama。
#[test]
fn test_voice_settings_persistence_serializes_consecutive_writes() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());

    state.select_agent_provider(AgentProviderKind::OpenAiCompatible);
    let first = state.take_pending_writes();
    state.select_agent_provider(AgentProviderKind::Ollama);
    let while_running = state.take_pending_writes();

    assert_eq!(first.len(), 1, "首批应只包含 OpenAI 选择写入");
    assert!(
        while_running.is_empty(),
        "首批未完成时不得启动第二个并行持久化任务"
    );
    for write in first {
        write.execute().expect("首批后台写入应成功");
    }
    state.record_persistence_result(Ok(()));
    let second = state.take_pending_writes();
    assert_eq!(second.len(), 1, "首批完成后应释放后续 Ollama 写入");
    for write in second {
        write.execute().expect("第二批后台写入应成功");
    }
    state.record_persistence_result(Ok(()));

    assert_eq!(
        store
            .get_string(super::provider_values::KEY_AGENT_PROVIDER)
            .expect("最终 Provider 查询应成功")
            .as_deref(),
        Some("ollama"),
        "串行执行后数据库必须保留最后一次选择"
    );
}
