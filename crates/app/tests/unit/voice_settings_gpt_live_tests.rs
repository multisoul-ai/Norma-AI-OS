use super::*;
use crate::settings::voice::{VoiceEntryState, VoiceRequirement, VoiceRequirementState};

/// AC12/AC13：真实设置存储中Live的模式、凭据引用与音色独立恢复，不借用其他供应商。
/// 数据构造：隔离SQLite，分别保存Live和Doubao账号引用，Live音色cedar。
/// 执行过程：通过正常load_from_store恢复设置，再核对两份配置和默认旧交互方式。
/// 预期结果：Live配置完整、豆包引用未覆盖，旧Hold行为仍可用；此项不等于真实App验收。
#[test]
fn gpt_live_configuration_loads_independently_from_existing_providers() {
    let temp = tempfile::tempdir().expect("隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    for (key, value) in [
        ("voice.desktop.mode", "gpt_live"),
        (
            "voice.gpt_live.credential_ref",
            "gpt-live-desktop-fixture-ref",
        ),
        ("voice.gpt_live.voice", "cedar"),
        ("voice.doubao.credential_ref", "doubao-fixture-ref"),
    ] {
        store.set_string(key, value).expect("隔离设置写入");
    }
    let state = VoiceSettingsState::load_from_store(store);
    assert_eq!(
        state.values().desktop_mode().as_str(),
        "gpt_live",
        "恢复独立Live模式"
    );
    assert_eq!(
        state.values().gpt_live_profile().credential_ref(),
        Some("gpt-live-desktop-fixture-ref"),
        "Live只能恢复自己的账号引用"
    );
    assert_eq!(
        state.values().gpt_live_profile().voice(),
        "cedar",
        "音色配置可恢复"
    );
    assert_eq!(
        state.values().doubao_credential_ref(),
        Some("doubao-fixture-ref"),
        "不改变豆包账号"
    );
    assert_eq!(
        state.values().dictation_trigger(),
        DictationTrigger::Hold,
        "旧快捷键默认不变"
    );
    assert!(
        !format!("{state:?}").contains("gpt-live-desktop-fixture-ref"),
        "Debug不展开凭据引用"
    );
}

/// AC12：Live Key沿用后台凭据写入机制，编辑器不回填secret，也不覆盖豆包引用。
/// 数据构造：隔离SQLite和仅用于本地测试的fake secret，预置独立豆包引用。
/// 执行过程：提交Live Key→确认前台未写库→执行pending writes→记录完成→正常重载。
/// 预期结果：Key只在独立凭据账号恢复，编辑器与Debug不泄露，豆包配置保持原值。
#[test]
fn gpt_live_key_is_persisted_in_the_background_and_editor_is_cleared() {
    let temp = tempfile::tempdir().expect("隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("voice.doubao.credential_ref", "doubao-fixture-ref")
        .expect("豆包设置");
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    let secret = "fixture-only-live-secret";
    state.begin_edit(VoiceSettingsField::GptLiveApiKey);
    state.auto_save_edit_value(VoiceSettingsField::GptLiveApiKey, secret.into());
    let account = state
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("独立凭据引用")
        .to_owned();
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &account).expect("查询隔离凭据"),
        None,
        "前台保存只排队，不能同步写SQLite"
    );
    assert!(
        state
            .editor_value(VoiceSettingsField::GptLiveApiKey)
            .is_empty(),
        "编辑器不回填secret"
    );
    assert!(
        !format!("{state:?}").contains(secret),
        "Debug不得泄漏secret"
    );
    for write in state.take_pending_writes() {
        write.execute().expect("后台写入");
    }
    state.record_persistence_result(Ok(()));
    let restored = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        restored.values().gpt_live_profile().credential_ref(),
        Some(account.as_str()),
        "正常重载恢复相同Live账号"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &account)
            .expect("恢复隔离凭据")
            .as_deref(),
        Some(secret),
        "仅在后台显式读取凭据值"
    );
    assert_eq!(
        restored.values().doubao_credential_ref(),
        Some("doubao-fixture-ref"),
        "不借用或覆盖豆包凭据"
    );
    assert_eq!(
        store
            .get_string("voice.gpt_live.api_key")
            .expect("不使用普通设置存Key"),
        None,
        "普通设置只保存引用"
    );
}

/// AC12：移除Live凭据也在后台执行，只清理对应账号，不触碰豆包配置。
/// 数据构造：隔离SQLite先保存fake Live Key和独立豆包引用。
/// 执行过程：正常保存→请求移除→检查前台未删库→依次执行引用与secret清理两个批次。
/// 预期结果：最终引用与对应secret均不存在，豆包配置保留；两个批次分别提交引用和清理旧账号。
#[test]
fn clearing_live_credential_removes_only_its_own_reference_and_account() {
    let temp = tempfile::tempdir().expect("隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("voice.doubao.credential_ref", "doubao-fixture-ref")
        .expect("豆包设置");
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-only-key".into());
    for write in state.take_pending_writes() {
        write.execute().expect("后台保存");
    }
    state.record_persistence_result(Ok(()));
    let account = state
        .values()
        .gpt_live_profile()
        .credential_ref()
        .expect("已保存引用")
        .to_owned();
    assert!(
        state.clear_gpt_live_credential(),
        "可移除已保存的独立Live凭据"
    );
    assert_eq!(
        store
            .get_string("voice.gpt_live.credential_ref")
            .expect("查询隔离设置")
            .as_deref(),
        Some(account.as_str()),
        "前台不能同步删除引用"
    );
    assert!(
        VoiceSettingsState::load_credential(&store, &account)
            .expect("查询隔离凭据")
            .is_some(),
        "前台不能同步删除secret"
    );
    // 第一批删除引用，确认成功后第二批清理旧账号；最多两个批次。
    for _ in 0..2 {
        for write in state.take_pending_writes() {
            write.execute().expect("后台清理");
        }
        state.record_persistence_result(Ok(()));
    }
    let restored = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        restored.values().gpt_live_profile().credential_ref(),
        None,
        "重载后Live未配置"
    );
    assert_eq!(
        VoiceSettingsState::load_credential(&store, &account).expect("清理结果"),
        None,
        "已移除账号不残留secret"
    );
    assert_eq!(
        restored.values().doubao_credential_ref(),
        Some("doubao-fixture-ref"),
        "不得移除其他供应商配置"
    );
}

/// AC12：批次在Live事务执行前失败时旧Key仍可用，不应发布候选账号。
/// 数据构造：隔离SQLite中一个可用旧账号；分别尝试换Key和移除；使用fake secrets。
/// 执行过程：取出但不执行Live事务，模拟前序步骤失败的宿主回调，再检查没有补偿写入并重载。
/// 预期结果：两条路径均保留旧引用和旧secret，未发布账号从未写入，不自动重试；底层真实SQL回滚另测。
#[test]
fn failed_live_credential_changes_preserve_the_previous_working_key() {
    for clear in [false, true] {
        let temp = tempfile::tempdir().expect("隔离设置目录");
        let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
        let mut state = VoiceSettingsState::load_from_store(store.clone());
        state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-old-key".into());
        for write in state.take_pending_writes() {
            write.execute().expect("保存旧Key");
        }
        state.record_persistence_result(Ok(()));
        let old = state
            .values()
            .gpt_live_profile()
            .credential_ref()
            .expect("旧引用")
            .to_owned();
        let unpublished = if clear {
            assert!(state.clear_gpt_live_credential(), "开始移除请求");
            None
        } else {
            state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-new-key".into());
            let new = state
                .values()
                .gpt_live_profile()
                .credential_ref()
                .expect("新引用")
                .to_owned();
            assert_ne!(new, old, "换Key不能先覆盖旧账号内容");
            Some(new)
        };
        let _unexecuted_batch = state.take_pending_writes();
        state.record_persistence_result(Err("fixture: reference write failed".into()));
        assert_eq!(
            state.values().gpt_live_profile().credential_ref(),
            Some(old.as_str()),
            "失败时前台恢复旧配置"
        );
        for write in state.take_pending_writes() {
            write.execute().expect("后台补偿");
        }
        state.record_persistence_result(Ok(()));
        let restored = VoiceSettingsState::load_from_store(store.clone());
        assert_eq!(
            restored.values().gpt_live_profile().credential_ref(),
            Some(old.as_str()),
            "重载仍恢复旧配置"
        );
        assert_eq!(
            VoiceSettingsState::load_credential(&store, &old)
                .expect("旧Key仍可读")
                .as_deref(),
            Some("fixture-old-key"),
            "失败不能损坏旧secret"
        );
        if let Some(account) = unpublished {
            assert_eq!(
                VoiceSettingsState::load_credential(&store, &account).expect("未发布账号清理"),
                None,
                "不能残留未发布的新secret"
            );
        }
        assert!(!state.settings_persistence_pending(), "补偿队列应收口");
    }
}

/// AC12：Live闲聊启动条件独立于旧ASR/TTS模型和项目后台，缺Key或麦克风仍阻止连接。
/// 数据构造：隔离库中的独立Live凭据，旧模型和后台均未配置；麦克风状态由宿主提供。
/// 执行过程：检查可启动→撤销麦克风→恢复麦克风并移除凭据引用。
/// 预期结果：仅Mic和Live凭据决定通话启动；此状态不代表真实云端认证或项目工具已就绪。
#[test]
fn live_conversation_can_connect_without_legacy_models_or_a_project_backend() {
    let temp = tempfile::tempdir().expect("隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("voice.desktop.mode", "gpt_live")
        .expect("Live模式");
    let mut state = VoiceSettingsState::load_from_store(store);
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-only-key".into());
    for write in state.take_pending_writes() {
        write.execute().expect("后台保存凭据");
    }
    state.record_persistence_result(Ok(()));
    state.set_microphone_requirement(VoiceRequirementState::Ready);
    assert_eq!(
        state.requirement_state(VoiceRequirement::AgentProvider),
        VoiceRequirementState::Missing,
        "未配置后台这一前提必须成立"
    );
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "闲聊不能被旧模型或缺后台阻断"
    );
    state.set_microphone_requirement(VoiceRequirementState::Missing);
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Unavailable {
            first_incomplete: VoiceRequirement::Microphone
        },
        "缺麦克风权限时不可启动"
    );
    state.set_microphone_requirement(VoiceRequirementState::Ready);
    assert!(state.clear_gpt_live_credential(), "移除独立Live凭据");
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Unavailable {
            first_incomplete: VoiceRequirement::AgentProvider
        },
        "缺Live Key时不可假报可启动"
    );
}

#[path = "voice_settings_gpt_live_concurrency_tests.rs"]
mod concurrency;

/// AC12：Live工具复用自身Key，不依赖全局供应商；凭据尚在保存时不可开始连接。
/// 数据构造：Live模式、麦克风已就绪；无旧模型、后台或VAD，提交fake Key。
/// 执行过程：检查保存中不可连接→完成后台提交→令旧后台失败→移除Live Key。
/// 预期结果：保存后独立Key即可准备工具；旧后台/VAD设置不影响Live；删除Key立即撤销。
#[test]
fn live_call_and_delegation_have_independent_readiness() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("voice.desktop.mode", "gpt_live")
        .expect("Live模式");
    let mut state = VoiceSettingsState::load_from_store(store);
    state.set_microphone_requirement(VoiceRequirementState::Ready);
    state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-key".into());
    assert_ne!(
        state.readiness(),
        VoiceEntryState::Ready,
        "凭据尚未发布不能连接"
    );
    assert!(
        !state.gpt_live_delegation_ready(),
        "尚未保存的Live Key不能用于工具调用"
    );
    for write in state.take_pending_writes() {
        write.execute().expect("凭据提交");
    }
    state.record_persistence_result(Ok(()));
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "Live闲聊不依赖工具准备"
    );
    assert!(
        state.gpt_live_delegation_ready(),
        "Live工具应直接复用已保存Key，不要求用户另配后台或手工安装VAD"
    );
    state.set_provider_requirement(VoiceRequirementState::Failed);
    assert!(
        state.gpt_live_delegation_ready(),
        "旧后台失效不能阻断独立Live工具"
    );
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "后台失效仍不阻断闲聊"
    );
    assert!(state.clear_gpt_live_credential(), "可移除Live Key");
    assert!(
        !state.gpt_live_delegation_ready(),
        "移除Key立即撤销工具准入"
    );
}

/// AC12：Live音色只接受官方marin/cedar，选择后后台保存且其他供应商配置不变。
/// 数据构造：隔离库默认marin，选择cedar，再传入非官方音色。
/// 执行过程：通过设置选择接口修改→后台落库→重载。
/// 预期结果：cedar可恢复；未知音色被拒绝且不会覆盖合法选择。
#[test]
fn live_voice_selection_validates_and_persists_independently() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    assert!(state.select_gpt_live_voice("cedar"), "可选择官方音色");
    assert!(
        !state.select_gpt_live_voice("alloy"),
        "不能借用Realtime的音色目录"
    );
    for write in state.take_pending_writes() {
        write.execute().expect("音色后台保存");
    }
    state.record_persistence_result(Ok(()));
    assert_eq!(
        VoiceSettingsState::load_from_store(store)
            .values()
            .gpt_live_profile()
            .voice(),
        "cedar",
        "合法音色持久化且不被无效值覆盖"
    );
}

/// AC12：真实SQLite写入错误经Live worker传回时，旧凭据保持完整并可显式重试。
/// 数据构造：隔离库中的可用旧Key；触发器拒绝Live引用的UPDATE或DELETE，分别覆盖保存和移除。
/// 执行过程：实际执行失败事务→宿主记录错误→核对旧数据与候选账号→移除触发器并重试。
/// 预期结果：失败没有部分提交；重试保存最新Key或移除旧Key，不自动无限重试。
#[test]
fn real_live_credential_worker_failure_rolls_back_and_can_retry() {
    for clear in [false, true] {
        let temp = tempfile::tempdir().expect("隔离目录");
        let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
        let mut state = VoiceSettingsState::load_from_store(store.clone());
        state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-old".into());
        for write in state.take_pending_writes() {
            write.execute().expect("保存旧凭据");
        }
        state.record_persistence_result(Ok(()));
        let old = state
            .values()
            .gpt_live_profile()
            .credential_ref()
            .expect("旧引用")
            .to_owned();
        let connection = rusqlite::Connection::open(store.path()).expect("失败夹具连接");
        let verb = if clear { "DELETE" } else { "UPDATE" };
        let row = if clear { "OLD" } else { "NEW" };
        connection.execute_batch(&format!("CREATE TRIGGER reject_live BEFORE {verb} ON settings WHEN {row}.key = 'voice.gpt_live.credential_ref' BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;")).expect("SQL真实失败夹具");
        if clear {
            assert!(state.clear_gpt_live_credential(), "移除请求");
        } else {
            state.commit_edit_value(VoiceSettingsField::GptLiveApiKey, "fixture-new".into());
        }
        let candidate = state
            .values()
            .gpt_live_profile()
            .credential_ref()
            .map(str::to_owned);
        let result = state
            .take_pending_writes()
            .into_iter()
            .try_for_each(PendingVoiceSettingWrite::execute);
        assert!(result.is_err(), "真实worker必须返回SQL失败");
        state.record_persistence_result(result);
        assert_eq!(
            VoiceSettingsState::load_from_store(store.clone())
                .values()
                .gpt_live_profile()
                .credential_ref(),
            Some(old.as_str()),
            "失败不能部分替换或移除引用"
        );
        assert_eq!(
            VoiceSettingsState::load_credential(&store, &old)
                .expect("旧secret")
                .as_deref(),
            Some("fixture-old"),
            "失败后旧secret完整"
        );
        if let Some(candidate) = candidate {
            assert_eq!(
                VoiceSettingsState::load_credential(&store, &candidate).expect("候选账号"),
                None,
                "事务失败不留孤儿secret"
            );
        }
        assert!(
            state.has_gpt_live_credential_retry(),
            "保存和Clear都保留可重试意图"
        );
        assert!(!state.settings_persistence_pending(), "失败不自动循环重试");
        connection
            .execute_batch("DROP TRIGGER reject_live;")
            .expect("恢复SQL写入");
        assert!(state.retry_gpt_live_credential_change(), "显式重试");
        for write in state.take_pending_writes() {
            write.execute().expect("真实重试提交");
        }
        state.record_persistence_result(Ok(()));
        let restored = VoiceSettingsState::load_from_store(store.clone());
        if clear {
            assert_eq!(
                restored.values().gpt_live_profile().credential_ref(),
                None,
                "Clear重试后引用消失"
            );
        } else {
            let account = restored
                .values()
                .gpt_live_profile()
                .credential_ref()
                .expect("新引用");
            assert_eq!(
                VoiceSettingsState::load_credential(&store, account)
                    .expect("新secret")
                    .as_deref(),
                Some("fixture-new"),
                "保存重试保留最新Key"
            );
        }
        assert_eq!(
            VoiceSettingsState::load_credential(&store, &old).expect("旧secret清理"),
            None,
            "成功后旧secret清理"
        );
    }
}
