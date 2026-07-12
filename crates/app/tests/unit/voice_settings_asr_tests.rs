use super::*;

/// VS-ASR-1：新 Voice Settings 必须用单次语音指令触发和共享模型 ID 表达语音行为。
///
/// 数据构造（含关键数值的推导过程）：
///   default mode       = single_voice_command，无总开关时的安全缺省
///   default trigger    = hold，与固定 ⌘E 按住行为一致
///   default model      = None，未安装 Ready 模型前不伪造选择
///   saved model count  = 1，Zipformer 不可变 ID 由两种模式共享
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve(empty saved/env) → SingleVoiceCommand + Hold + no model
///   2. resolve(saved ContinuousListening + Toggle + Zipformer) → 读回精确值
///
/// 预期结果：
///   - 正断言：已保存的模式、触发和 Zipformer ID 被保留
///   - 负断言：缺省状态不能启用持续聆听或伪造模型选择
#[test]
fn test_voice_settings_resolve_new_interaction_mode_trigger_and_shared_model_id() {
    let defaults =
        VoiceSettingsValues::resolve(&SavedVoiceSettings::default(), &EnvVoiceSettings::default());

    assert_eq!(
        defaults.interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "新 Settings 缺省必须为单次语音指令，不应自动启动持续聆听"
    );
    assert_eq!(
        defaults.dictation_trigger(),
        DictationTrigger::Hold,
        "单次语音指令缺省必须保留按住 ⌘E 语义"
    );
    assert!(
        defaults.asr_model_id().is_none(),
        "没有 Ready 安装时 Settings 不应伪造当前识别模型"
    );

    let zipformer_id = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID)
        .expect("内置 Zipformer ID 应有效");
    let saved = SavedVoiceSettings {
        interaction_mode: Some(VoiceInteractionMode::ContinuousListening),
        dictation_trigger: Some(DictationTrigger::Toggle),
        asr_model_id: Some(zipformer_id.clone()),
        ..SavedVoiceSettings::default()
    };
    let resolved = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());

    assert_eq!(
        resolved.interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "已保存的持续聆听模式应被精确读回"
    );
    assert_eq!(
        resolved.dictation_trigger(),
        DictationTrigger::Toggle,
        "已保存的切换式听写触发应被精确读回"
    );
    assert_eq!(
        resolved.asr_model_id(),
        Some(&zipformer_id),
        "两种语音交互模式应共享同一个不可变 Zipformer ID"
    );
    assert_ne!(
        resolved.interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "已保存持续聆听时不应退回单次语音指令缺省值"
    );
}

/// VS-ASR-1B：升级用户的 Dictation 设置必须迁移为单次语音指令与新 trigger key。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy mode keys     = 1（voice.interaction.mode = dictation）
///   legacy trigger keys  = 1（voice.dictation.trigger = toggle）
///   canonical writes     = 3（新 mode + 新 trigger + 删除旧 trigger）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向隔离 SQLite 写入旧 Dictation 值和旧 trigger key
///   2. load_from_store 解析旧值为 SingleVoiceCommand / Toggle
///   3. 执行挂起写入后，从 SQLite 读取新 key 和旧 key
///
/// 预期结果：
///   - 正断言：内存与新 key 精确表达 SingleVoiceCommand / Toggle
///   - 负断言：旧 trigger key 不得在迁移后继续存在
#[test]
fn test_legacy_dictation_values_migrate_to_single_voice_command_keys() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的 Settings SQLite");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string(super::values::KEY_INTERACTION_MODE, "dictation")
        .expect("应写入旧 Dictation mode 作为升级输入");
    store
        .set_string(super::values::LEGACY_KEY_DICTATION_TRIGGER, "toggle")
        .expect("应写入旧 Dictation trigger 作为升级输入");

    let mut state = VoiceSettingsState::load_from_store(store.clone());

    assert_eq!(
        state.values().interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "旧 dictation 值必须在内存中迁移为单次语音指令"
    );
    assert_eq!(
        state.values().dictation_trigger(),
        DictationTrigger::Toggle,
        "旧 toggle 值必须保留为单次语音指令的 Toggle 偏好"
    );
    let writes = state.take_pending_writes();
    assert_eq!(
        writes.len(),
        3,
        "迁移必须写入两个新 key 并删除一个旧 trigger key"
    );
    for write in writes {
        write.execute().expect("迁移写入应可在后台完成");
    }

    assert_eq!(
        store
            .get_string(super::values::KEY_INTERACTION_MODE)
            .expect("读取迁移后的 mode key 应成功"),
        Some("single_voice_command".to_string()),
        "迁移后 mode key 必须保存新的稳定值"
    );
    assert_eq!(
        store
            .get_string(super::values::KEY_SINGLE_VOICE_COMMAND_TRIGGER)
            .expect("读取迁移后的 trigger key 应成功"),
        Some("toggle".to_string()),
        "迁移后 trigger 必须保存到单次语音指令 key"
    );
    assert!(
        store
            .get_string(super::values::LEGACY_KEY_DICTATION_TRIGGER)
            .expect("读取旧 trigger key 应成功")
            .is_none(),
        "迁移后旧 Dictation trigger key 不得残留"
    );
}

/// VS-ASR-2：模式、听写触发和共享模型选择必须通过后台写队列持久化。
///
/// 数据构造（含关键数值的推导过程）：
///   setting changes = 3（mode + trigger + model）
///   expected writes = 3（每个稳定 key 各一次）
///   selected model  = zipformer-streaming-zh-int8-2025-06-30
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从空 SQLite SettingsStore 加载状态 → 三个新 key 均不存在
///   2. 选择 ContinuousListening、Toggle 和 Zipformer → 只更新内存并排队写入
///   3. 执行三个 pending writes → 再次加载并解析稳定值
///
/// 预期结果：
///   - 正断言：后台执行后精确读回三个设置
///   - 负断言：执行 pending writes 前 SQLite 中不存在模型选择
#[test]
fn test_voice_interaction_and_model_selection_persist_through_deferred_writes() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut state = VoiceSettingsState::load_from_store(store.clone());
    let zipformer_id = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID)
        .expect("内置 Zipformer ID 应有效");

    state.select_interaction_mode(VoiceInteractionMode::ContinuousListening);
    state.select_dictation_trigger(DictationTrigger::Toggle);
    state.select_asr_model(Some(zipformer_id.clone()));

    assert!(
        store
            .get_string(super::values::KEY_ASR_MODEL_ID)
            .expect("后台写入前查询应成功")
            .is_none(),
        "选择模型不得在 GPUI 状态变更路径同步写 SQLite"
    );
    let writes = state.take_pending_writes();
    assert_eq!(writes.len(), 3, "三个不同设置变化应各保留一个后台写操作");
    for write in writes {
        write.execute().expect("模拟后台持久化应成功");
    }

    let reloaded = VoiceSettingsState::load_from_store(store);
    assert_eq!(
        reloaded.values().interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "重新加载后应保留持续聆听模式"
    );
    assert_eq!(
        reloaded.values().dictation_trigger(),
        DictationTrigger::Toggle,
        "重新加载后应保留切换式听写触发"
    );
    assert_eq!(
        reloaded.values().asr_model_id(),
        Some(&zipformer_id),
        "重新加载后应保留共享 Zipformer 模型选择"
    );
    assert_ne!(
        reloaded.values().interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "已持久化持续聆听后不应退回默认单次语音指令模式"
    );
}

/// VS-ASR-3：新架构必须忽略旧 Paraformer 路径与 Always Listening 布尔设置。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy keys = 2（voice.paraformer.model_dir + voice.realtime.enabled）
///   canonical voice keys = 0（mode / trigger / model ID 均未保存）
///   preserved Agent keys = 2（provider + model）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 SQLite 写入两个旧语音 key 和两个 Agent key
///   2. 通过 VoiceSettingsState::load_from_store 加载新架构设置
///   3. 读取交互模式、模型 ID 和 Agent profile
///
/// 预期结果：
///   - 正断言：语音设置回到 SingleVoiceCommand / no model，Agent 配置仍保留
///   - 负断言：旧 realtime=true 不得隐式启用 ContinuousListening
#[test]
fn test_legacy_asr_and_realtime_keys_are_ignored_while_agent_settings_survive() {
    let temp = tempfile::tempdir().expect("测试应创建临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    store
        .set_string("voice.paraformer.model_dir", "/legacy/model")
        .expect("应写入旧模型路径用于迁移边界测试");
    store
        .set_string("voice.realtime.enabled", "true")
        .expect("应写入旧持续聆听布尔值用于迁移边界测试");
    store
        .set_string(super::provider_values::KEY_AGENT_PROVIDER, "ollama")
        .expect("应写入需保留的 Agent Provider");
    store
        .set_string(
            super::provider_values::KEY_AGENT_OLLAMA_MODEL,
            "qwen-preserved",
        )
        .expect("应写入需保留的 Agent model");

    let state = VoiceSettingsState::load_from_store(store);

    assert_eq!(
        state.values().interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "缺少新 mode key 时必须回到单次语音指令"
    );
    assert!(
        state.values().asr_model_id().is_none(),
        "旧模型目录不得被迁移或伪造成模型 ID"
    );
    assert_eq!(
        state.values().active_provider_profile().model(),
        "qwen-preserved",
        "ASR 架构切换不得清除独立 Agent 模型配置"
    );
    assert_ne!(
        state.values().interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "旧 realtime=true 不得启用新持续聆听模式"
    );
}
