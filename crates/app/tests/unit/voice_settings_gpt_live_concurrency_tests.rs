use super::*;

/// AC12：两个项目实例共享设置时，迟到的失败不能覆盖另一窗口已成功保存的Key。
/// 数据构造：同一隔离库的旧Key，A/B从同一引用加载；B成功替换，A的失败回调迟到。
/// 执行过程：挂起A写入→完成B和清理→向A报告失败→执行A补偿→重载共享配置。
/// 预期结果：全局仍指向B的有效secret；A不能恢复已被B删除的旧账号。
#[test]
fn failed_live_key_write_cannot_undo_another_window_success() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut first = VoiceSettingsState::load_from_store(store.clone());
    first.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-old".into());
    for write in first.take_pending_writes() {
        write.execute().expect("旧Key");
    }
    first.record_persistence_result(Ok(()));
    let mut second = VoiceSettingsState::load_from_store(store.clone());
    first.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-a".into());
    let _failed_batch = first.take_pending_writes();
    second.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-b".into());
    for _ in 0..2 {
        for write in second.take_pending_writes() {
            write.execute().expect("B保存/清理");
        }
        second.record_persistence_result(Ok(()));
    }
    let expected = second
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("B引用")
        .to_owned();
    first.record_persistence_result(Err("fixture: A failed before commit".into()));
    for write in first.take_pending_writes() {
        write.execute().expect("A后续清理");
    }
    first.record_persistence_result(Ok(()));
    let restored = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        restored.values().gpt_live_profile().credential_ref(),
        Some(expected.as_str()),
        "A失败不得覆盖B成功配置"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &expected)
            .expect("B凭据")
            .as_deref(),
        Some("fixture-b"),
        "B已发布的secret仍可使用"
    );
}

/// AC12：快速输入和写入期间继续输入均须保留最新完整Key，不能只保存首字符。
/// 数据构造：隔离库；先模拟s→sk→sk-full，再在批次执行中输入另一个完整值。
/// 执行过程：按编辑器autosave接口连续更新，执行两个后台批次后重载凭据。
/// 预期结果：第一批保存sk-full，第二批保存最新值；所有已替换账号均清理。
#[test]
fn live_key_autosave_keeps_latest_input_before_and_during_a_write() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.begin_edit(VoiceSettingsField::GptLiveApiKey);
    for value in ["s", "sk", "sk-full"] {
        assert!(
            state
                .auto_save_edit_value(VoiceSettingsField::GptLiveApiKey, value.into())
                .is_some(),
            "每次编辑都应接受最新草稿"
        );
    }
    let first = state
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("首批账号")
        .to_owned();
    let batch = state.take_pending_writes();
    state.auto_save_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-final".into());
    for write in batch {
        write.execute().expect("首批后台保存");
    }
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &first)
            .expect("首批Key")
            .as_deref(),
        Some("sk-full"),
        "快速输入需合并完整值"
    );
    state.record_persistence_result(Ok(()));
    for write in state.take_pending_writes() {
        write.execute().expect("后续草稿后台保存");
    }
    state.record_persistence_result(Ok(()));
    let restored = VoiceSettingsState::load_from_store(store.clone());
    let latest = restored
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("最新账号");
    assert_eq!(
        VoiceSettingsState::load_credential(&store, latest)
            .expect("完整Key")
            .as_deref(),
        Some("fixture-final"),
        "运行中编辑不能丢失"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &first).expect("旧账号清理"),
        None,
        "旧secret应原子清理"
    );
    assert!(!state.settings_persistence_pending(), "正常保存队列应收口");
}

/// AC12：过期保存需返回当前配置且零写入；Live自身成功不能被同批其他失败误回滚。
/// 数据构造：共享库A/B从同一旧引用加载，B先成功；另测A提交成功后的聚合错误。
/// 执行过程：B提交后执行A过期批次，检查结果；再提交有效Key并报告后续无关写入失败。
/// 预期结果：冲突没有孤儿secret，A内存追上B；已成功的Live引用在聚合错误后仍有效。
#[test]
fn live_key_transaction_reports_conflict_and_its_own_success() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut first = VoiceSettingsState::load_from_store(store.clone());
    let mut second = VoiceSettingsState::load_from_store(store.clone());
    first.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-a".into());
    let unpublished = first
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("A候选")
        .to_owned();
    let stale = first.take_pending_writes();
    second.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-b".into());
    for write in second.take_pending_writes() {
        write.execute().expect("B提交");
    }
    second.record_persistence_result(Ok(()));
    let result = stale
        .into_iter()
        .try_for_each(PendingVoiceSettingWrite::execute);
    assert!(result.is_err(), "过期A事务必须冲突");
    first.record_persistence_result(result);
    assert_eq!(
        first.values().gpt_live_profile().credential_ref(),
        second.values().gpt_live_profile().credential_ref(),
        "冲突结果要带回当前引用"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &unpublished).expect("未发布账号"),
        None,
        "冲突不能部分保存secret"
    );
    first.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-c".into());
    for write in first.take_pending_writes() {
        write.execute().expect("Live自身提交成功");
    }
    first.record_persistence_result(Err("fixture: later unrelated write failed".into()));
    let account = first
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("已提交引用");
    assert_eq!(
        VoiceSettingsState::load_credential(&store, account)
            .expect("Live自身结果")
            .as_deref(),
        Some("fixture-c"),
        "聚合失败不能恢复旧引用"
    );
    assert!(!first.settings_persistence_pending(), "失败不自动补偿/重试");
}

/// AC12：清除优先于较早的编辑草稿，写入期间的Clear不会导致旧Key自动保存回来。
/// 数据构造：一个正在写入的LiveKey，后续Save和Clear依次到达。
/// 执行过程：执行首批→处理回调→执行Clear批次→重载。
/// 预期结果：引用和两份候选secret均不存在，后台队列结束。
#[test]
fn clearing_during_a_live_key_write_supersedes_queued_input() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-first".into());
    let account = state
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("首批引用")
        .to_owned();
    let batch = state.take_pending_writes();
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-later".into());
    assert!(
        state.clear_gpt_live_credential(),
        "运行中Clear可成为最新意图"
    );
    for write in batch {
        write.execute().expect("首批提交");
    }
    state.record_persistence_result(Ok(()));
    for write in state.take_pending_writes() {
        write.execute().expect("清除提交");
    }
    state.record_persistence_result(Ok(()));
    assert_eq!(
        VoiceSettingsState::load_from_store(store.clone())
            .values()
            .gpt_live_profile()
            .credential_ref(),
        None,
        "Clear之后不可自动恢复Key"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &account).expect("secret清理"),
        None,
        "已清除secret不存在"
    );
    assert!(
        !state.credential_persistence_pending(),
        "清除完成后不再有凭据pending"
    );
}

/// AC12：迟到的Live保存失败不能劫持当前字段编辑，最新Key应可显式重试。
/// 数据构造：Live首批挂起，继续输入完整Key，然后切换到BaseUrl字段；后台模拟失败。
/// 执行过程：报告Live错误→继续编辑BaseUrl→通过明确重试入口提交最新Key。
/// 预期结果：当前字段不变且可保存，重试使用最后完整Key，不回填secret或自动循环。
#[test]
fn failed_live_key_does_not_steal_editing_focus_and_latest_value_can_retry() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-first".into());
    let _failed = state.take_pending_writes();
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-latest".into());
    state.begin_edit_with_value(VoiceSettingsField::BaseUrl, "http://localhost:11434".into());
    state.record_persistence_result(Err("fixture: IO failure".into()));
    assert_eq!(
        state.editing_field(),
        Some(VoiceSettingsField::BaseUrl),
        "迟到失败不能改变当前编辑字段"
    );
    assert!(
        state
            .auto_save_edit_value(VoiceSettingsField::BaseUrl, "http://localhost:11435".into())
            .is_some(),
        "当前字段仍可保存"
    );
    assert!(
        !state.credential_persistence_pending(),
        "失败不能卡住凭据或自动重试"
    );
}

/// AC12：显式重试保存最后完整Key，不暴露到编辑器，且不阻止当前其他字段编辑。
/// 数据构造：一个失败的运行中批次和一个后续完整Key，当前编辑BaseUrl。
/// 执行过程：模拟失败→点击重试语义接口→执行正常后台批次→读取引用对应secret。
/// 预期结果：最新Key保存，重试入口消失；当前编辑字段保持不变，Debug不含secret。
#[test]
fn explicit_live_credential_retry_uses_latest_value_without_refilling_editor() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-first".into());
    let _failed = state.take_pending_writes();
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-latest".into());
    state.begin_edit_with_value(VoiceSettingsField::BaseUrl, "http://localhost".into());
    state.record_persistence_result(Err("fixture: IO failure".into()));
    assert!(state.has_gpt_live_credential_retry(), "失败应提供显式重试");
    assert!(
        state.retry_gpt_live_credential_change(),
        "可提交最后完整意图"
    );
    for write in state.take_pending_writes() {
        write.execute().expect("重试保存");
    }
    state.record_persistence_result(Ok(()));
    let account = state
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("已保存Key");
    assert_eq!(
        VoiceSettingsState::load_credential(&store, account)
            .expect("最新secret")
            .as_deref(),
        Some("fixture-latest"),
        "重试保存最后完整值"
    );
    assert_eq!(
        state.editing_field(),
        Some(VoiceSettingsField::BaseUrl),
        "重试不能切换当前编辑"
    );
    assert!(
        !state.has_gpt_live_credential_retry(),
        "成功后清除待重试草稿"
    );
    assert!(
        state
            .editor_value(VoiceSettingsField::GptLiveApiKey)
            .is_empty(),
        "secret不回填输入框"
    );
    assert!(
        !format!("{state:?}").contains("fixture-latest"),
        "重试草稿也不进入Debug"
    );
}
