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

/// VS-ASR-6：旧 Azure profile 的迟到验证成功不得把已编辑的新草稿标记 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   original profile = eastus + zh-CN + credential-old
///   edited profile = westus2 + en-US + 同一 credential
///   validation completions = 1（只完成 original）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 original Draft 并保存其不可变快照
///   2. 在请求返回前把 region/locale 改成 edited Draft
///   3. 尝试把 original 的成功结果写回当前状态
///
/// 预期结果：
///   - 正断言：写回被拒绝，当前 profile 仍是 westus2/en-US
///   - 负断言：未经验证的新草稿不得进入 Ready 或激活 Azure
#[test]
fn test_stale_azure_validation_cannot_mark_edited_profile_ready() {
    let saved = SavedVoiceSettings {
        azure_asr_region: Some("eastus".to_string()),
        azure_asr_locale: Some("zh-CN".to_string()),
        azure_asr_credential_ref: Some("credential-old".to_string()),
        azure_asr_ready: Some(false),
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
    let validated_profile = state.values().azure_asr_profile().clone();

    state.update_azure_asr_draft("westus2", "en-US", Some("credential-old".to_string()));
    let accepted = state.record_azure_asr_validation_result(&validated_profile, Ok(()));

    assert!(
        !accepted,
        "旧 profile 的迟到验证结果必须被 generation/fingerprint 拒绝"
    );
    assert_eq!(
        state.values().azure_asr_profile().region(),
        "westus2",
        "拒绝迟到结果后必须保留用户最新 region"
    );
    assert_eq!(
        state.values().azure_asr_profile().locale(),
        "en-US",
        "拒绝迟到结果后必须保留用户最新 locale"
    );
    assert!(
        !state.values().azure_asr_profile().is_ready(),
        "未经验证的新 Azure 草稿不得被旧成功结果标记 Ready"
    );
    assert!(
        !state.activate_azure_asr(),
        "未经验证的新 Azure 草稿不得成为活动 Provider"
    );
}

/// VS-ASR-4：升级后必须默认保留 Local Sherpa，并独立保存 Azure 草稿。
///
/// 数据构造（含关键数值的推导过程）：
///   provider keys       = 0，模拟升级前没有多 Provider 设置
///   local model ids     = 1，保留现有 Zipformer 选择
///   Azure draft fields  = 3（region + locale + credential reference）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve 旧设置 → 当前 Provider 缺省为 Local Sherpa
///   2. 保存 Azure 草稿但不激活 → 当前 Provider 仍为 Local Sherpa
///   3. 将草稿标记 Ready 并显式激活 → 当前 Provider 原子切换为 Azure
///
/// 预期结果：
///   - 正断言：Azure profile 与原 Local model 都被保留
///   - 负断言：仅编辑 Azure 草稿不得隐式切换当前 Provider
#[test]
fn test_azure_draft_does_not_replace_local_until_ready_and_explicitly_activated() {
    let local_model = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID)
        .expect("内置 Zipformer ID 应有效");
    let saved = SavedVoiceSettings {
        asr_model_id: Some(local_model.clone()),
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());

    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "缺少新 Provider key 的升级用户必须继续使用 Local Sherpa"
    );

    state.update_azure_asr_draft("eastus", "zh-CN", Some("azure-test-ref".to_string()));

    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "编辑 Azure 草稿不得隐式替换当前 Local Sherpa"
    );
    assert_eq!(
        state.values().azure_asr_profile().region(),
        "eastus",
        "Azure 草稿必须独立保留 region"
    );
    assert_eq!(
        state.values().asr_model_id(),
        Some(&local_model),
        "编辑 Azure 草稿不得清除 Local Sherpa 模型选择"
    );
    assert!(
        !state.activate_azure_asr(),
        "未验证 Ready 的 Azure 草稿不得被激活"
    );

    state.mark_azure_asr_ready(true);

    assert!(state.activate_azure_asr(), "Ready 后的显式激活动作应成功");
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::CloudAzure,
        "显式激活后当前 Provider 必须原子切换为 Azure"
    );
    assert_ne!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "Azure 激活成功后不得仍报告 Local Sherpa"
    );
}

/// VS-ASR-5：Azure locale 必须来自内置目录且缺省为 zh-CN。
///
/// 数据构造（含关键数值的推导过程）：
///   default locale      = zh-CN（中文用户缺省）
///   accepted locale     = en-US（目录内不同语言）
///   rejected locale     = xx-NOT-REAL（目录外值）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve 空设置 → Azure profile 使用 zh-CN
///   2. 查询内置目录并选择 en-US → 校验成功
///   3. 校验目录外 locale → 明确拒绝
///
/// 预期结果：
///   - 正断言：zh-CN 与 en-US 均存在且可选择
///   - 负断言：目录外 locale 不得进入可激活 profile
#[test]
fn test_azure_locale_catalog_defaults_to_zh_cn_and_rejects_unknown_locale() {
    let values =
        VoiceSettingsValues::resolve(&SavedVoiceSettings::default(), &EnvVoiceSettings::default());

    assert_eq!(
        values.azure_asr_profile().locale(),
        "zh-CN",
        "Azure locale 缺省必须为 zh-CN"
    );
    assert!(
        asr::azure_speech_locales().contains(&"zh-CN"),
        "内置 Azure locale 目录必须包含 zh-CN"
    );
    assert!(
        asr::azure_speech_locales().contains(&"en-US"),
        "内置 Azure locale 目录必须包含 en-US"
    );
    assert!(
        asr::validate_azure_speech_locale("en-US").is_ok(),
        "目录内 en-US 必须通过 typed profile 校验"
    );
    assert!(
        asr::validate_azure_speech_locale("xx-NOT-REAL").is_err(),
        "目录外 locale 必须被拒绝，不能自由输入任意值"
    );
}

/// VS-ASR-6：Azure Keychain 写入失败必须恢复旧引用并撤销 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   previous reference  = azure-old-account（1 个可恢复引用）
///   replacement secret  = fake-new-secret（只进入待执行 Keychain 写）
///   persistence result  = Err 1 次（模拟 Keychain 拒绝）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以旧引用 + Ready profile 构造 Azure 设置
///   2. 提交替换密钥 → 内存暂时指向新 opaque reference
///   3. 注入后台持久化失败 → 回滚到旧引用并撤销 Ready
///
/// 预期结果：
///   - 正断言：旧引用被恢复，可供用户重试
///   - 负断言：失败的新引用不得保持 Ready 或激活 Azure
#[test]
fn test_failed_azure_keychain_replacement_restores_previous_reference() {
    let saved = SavedVoiceSettings {
        asr_provider: Some(asr::AsrProviderId::LocalSherpa),
        azure_asr_region: Some("eastus".to_string()),
        azure_asr_locale: Some("zh-CN".to_string()),
        azure_asr_credential_ref: Some("azure-old-account".to_string()),
        azure_asr_ready: Some(true),
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());

    state.save_azure_asr_credential("fake-new-secret".to_string());
    state.record_persistence_result(Err("simulated Keychain denial".to_string()));

    assert_eq!(
        state.values().azure_asr_profile().credential_ref(),
        Some("azure-old-account"),
        "Azure Keychain 替换失败后必须恢复原 opaque reference"
    );
    assert!(
        !state.values().azure_asr_profile().is_ready(),
        "替换失败后 Azure profile 不得继续声称 Ready"
    );
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "失败不得隐式激活 Azure 或改变当前 Local Provider"
    );
}
