use super::*;

/// VS-D1：Setup 的来源视图、模型目录与 Ready Advanced 必须互不影响。
///
/// 数据构造（含关键数值的推导过程）：
///   Setup ASR 视图 = active source(1) + On-device(1) + Azure Speech(1)
///   Setup 目的地 = Overview(1) / AsrOnDevice(1) / AsrModels(1) / AsrAzure(1) / SpokenReplies(1)
///   初始 Advanced 展开状态 = false
///   Advanced 展开操作 = 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 VoiceSettingsState → 两个区域都折叠
///   2. 打开 Azure Speech → Azure 配置可见、On-device 目录保持折叠、当前 Provider 不变
///   3. 回到 On-device 并展开 AsrModels → Advanced 仍折叠
///   4. 展开 Ready Advanced → Setup 仍指向 AsrModels
///   5. 切换至 SpokenReplies → ASR 目录折叠，TTS 焦点成立
///   6. 结束 Setup review → 目的地恢复 Overview，Advanced 状态不被改写
///
/// 预期结果：
///   - 正断言：On-device、Azure Speech、ASR 模型和 Spoken Replies 都能成为明确目的地
///   - 负断言：展开 Setup 不得连带展开 Advanced
///   - 负断言：浏览 Azure 配置不得激活 Azure 或同时展开本地模型目录
///   - 负断言：Spoken Replies 目的地不得同时展开 ASR；离开 Setup 后不得残留目的地
#[test]
fn test_setup_model_disclosure_is_independent_from_ready_advanced() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    assert!(
        !state.setup_models_expanded(),
        "默认进入 Voice Setup 时，其他模型目录必须保持折叠"
    );
    assert!(
        !state.advanced_expanded(),
        "默认进入 Ready 页面时，Advanced 必须保持折叠"
    );

    state.open_setup_destination(VoiceSetupDestination::AsrAzure);
    assert_eq!(
        state.setup_asr_source(),
        asr::AsrProviderId::CloudAzure,
        "打开 Azure Speech 配置后，Setup 必须只投影 Azure 来源视图"
    );
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::LocalSherpa,
        "浏览 Azure 配置不得隐式替换当前 Local Sherpa Provider"
    );
    assert!(
        !state.setup_models_expanded(),
        "Azure 配置可见时不得同时铺开 On-device 模型目录"
    );

    state.open_setup_destination(VoiceSetupDestination::AsrOnDevice);
    assert_eq!(
        state.setup_asr_source(),
        asr::AsrProviderId::LocalSherpa,
        "返回 On-device 后，Setup 必须恢复本地来源视图"
    );
    assert!(
        !state.setup_models_expanded(),
        "仅浏览 On-device 摘要时，模型目录必须保持折叠"
    );

    state.toggle_setup_models_expanded();
    assert!(
        state.setup_models_expanded(),
        "用户点击 Choose Other Models 后，Setup 模型目录必须展开"
    );
    assert!(
        !state.advanced_expanded(),
        "展开 Setup 模型目录不得连带展开 Ready Advanced"
    );

    state.toggle_advanced_expanded();
    assert!(state.advanced_expanded(), "Ready Advanced 应能独立展开");
    assert!(
        state.setup_models_expanded(),
        "展开 Ready Advanced 不得关闭已展开的 Setup 模型目录"
    );

    state.open_setup_destination(VoiceSetupDestination::SpokenReplies);
    assert!(
        state.setup_focuses_spoken_replies(),
        "Spoken Replies Change 必须记录明确的 TTS 目的地"
    );
    assert!(
        !state.setup_models_expanded(),
        "Spoken Replies 成为目的地时不得同时展开 ASR 目录"
    );

    state.set_reviewing_setup(false);
    assert!(
        !state.setup_models_expanded(),
        "结束 Setup review 后，其他模型目录必须恢复默认折叠"
    );
    assert!(
        state.advanced_expanded(),
        "结束 Setup review 不得改写 Ready Advanced 的独立状态"
    );
}

/// VS-D2：下载额外 On-device 模型只标记 Ready，首次安装才自动成为当前模型。
///
/// 数据构造（含关键数值的推导过程）：
///   已安装模型 = Paraformer Bilingual(1)
///   新下载模型 = Zipformer Bilingual(1)
///   fresh state 当前模型 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在已有 Paraformer 选择的 state 中完成 Zipformer 下载 → Zipformer 标记 Ready
///   2. 检查当前选择仍为 Paraformer → 后台下载不得打断现有识别配置
///   3. 在无当前模型的 fresh state 中完成同一 Zipformer 下载 → 自动建立首次选择
///
/// 预期结果：
///   - 正断言：两次下载完成后 Zipformer 都进入 Ready
///   - 正断言：首次安装会选中 Zipformer，使新用户能继续完成 Setup
///   - 负断言：已有模型时不得自动切换到刚下载的 Zipformer
#[test]
fn test_completed_download_preserves_existing_model_but_selects_first_install() {
    let paraformer = asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID)
        .expect("built-in Paraformer model id should parse");
    let zipformer = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("built-in Zipformer model id should parse");
    let mut configured = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    configured.model_manager_mut().mark_ready(&paraformer);
    configured.select_asr_model(Some(paraformer.clone()));

    let switched_existing = configured.complete_asr_model_install(zipformer.clone());

    assert!(
        matches!(
            configured.model_manager().installation(&zipformer),
            Some(super::super::models::AsrModelInstallState::Ready)
        ),
        "completed alternative download must expose Zipformer as Ready"
    );
    assert_eq!(
        configured.values().asr_model_id(),
        Some(&paraformer),
        "completed alternative download must preserve the existing Paraformer selection"
    );
    assert!(
        !switched_existing,
        "completed alternative download must report that it did not activate a new model"
    );

    let mut fresh = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let selected_first = fresh.complete_asr_model_install(zipformer.clone());

    assert!(
        matches!(
            fresh.model_manager().installation(&zipformer),
            Some(super::super::models::AsrModelInstallState::Ready)
        ),
        "first completed download must expose Zipformer as Ready"
    );
    assert_eq!(
        fresh.values().asr_model_id(),
        Some(&zipformer),
        "first completed download must select Zipformer so Setup can advance"
    );
    assert!(
        selected_first,
        "first completed download must report that it established the initial model selection"
    );
}

/// VS-D3：只有活动 On-device 来源切换到另一个已验证模型时才需要停止当前识别。
///
/// 数据构造（含关键数值的推导过程）：
///   当前模型 = Paraformer Bilingual(1)
///   候选模型 = Zipformer Bilingual(1)
///   Provider 状态 = Azure active(1) / Local active(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 激活已验证 Azure，并请求切换到 Zipformer → 当前云端会话不受本地选择影响
///   2. 回到 Local，并请求继续使用 Paraformer → 同模型选择不需要重启
///   3. Local 请求切换到 Zipformer → 实际 recognizer 模型发生变化，需要重启
///
/// 预期结果：
///   - 负断言：Azure 活动时不得因本地模型 Use 停止识别
///   - 负断言：Local 重选当前模型不得停止识别
///   - 正断言：Local 切换到不同模型必须停止旧 recognizer
#[test]
fn test_model_switch_restart_is_limited_to_changed_active_local_model() {
    let paraformer = asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID)
        .expect("built-in Paraformer model id should parse");
    let zipformer = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("built-in Zipformer model id should parse");
    let saved = SavedVoiceSettings {
        asr_model_id: Some(paraformer.clone()),
        ..SavedVoiceSettings::default()
    };
    let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
    state.update_azure_asr_draft("eastus", "zh-CN", Some("test-key-ref".to_string()));
    state.mark_azure_asr_ready(true);
    assert!(
        state.activate_azure_asr(),
        "test setup must activate a validated Azure profile"
    );

    assert!(
        !state.should_restart_for_asr_model_selection(&zipformer),
        "Azure recognition must keep running while a local model is selected"
    );

    state.activate_local_sherpa_asr();
    assert!(
        !state.should_restart_for_asr_model_selection(&paraformer),
        "reselecting the current local model must not restart recognition"
    );
    assert!(
        state.should_restart_for_asr_model_selection(&zipformer),
        "switching an active local recognizer to a different model must request restart"
    );
}

/// VS-D4：并发模型 Use 请求只接受最后一次校验结果。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 generation = 0
///   Use B 请求 = generation 1
///   Use C 请求 = generation 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 开始 Use B → generation 从 0 增至 1
///   2. 在 B 完成前开始 Use C → generation 从 1 增至 2
///   3. 分别检查 B、C 的迟到结果是否仍可提交
///
/// 预期结果：
///   - 负断言：较早的 B generation 不得覆盖 C
///   - 正断言：最新的 C generation 必须仍可提交
#[test]
fn test_only_latest_asr_model_selection_generation_is_accepted() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    let use_b = state.begin_asr_model_selection();
    let use_c = state.begin_asr_model_selection();

    assert!(
        !state.accepts_asr_model_selection_generation(use_b),
        "an older model selection validation must not overwrite the latest request"
    );
    assert!(
        state.accepts_asr_model_selection_generation(use_c),
        "the latest model selection validation must remain eligible to commit"
    );
}
