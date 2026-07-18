use super::*;

/// VS-D1：Setup 的语义目的地与 Ready Advanced 必须互不影响。
///
/// 数据构造（含关键数值的推导过程）：
///   Setup 目的地 = Overview(1) / AsrModels(1) / SpokenReplies(1)
///   初始 Advanced 展开状态 = false
///   Advanced 展开操作 = 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 VoiceSettingsState → 两个区域都折叠
///   2. 切换至 AsrModels → Advanced 仍折叠
///   3. 展开 Ready Advanced → Setup 仍指向 AsrModels
///   4. 切换至 SpokenReplies → ASR 目录折叠，TTS 焦点成立
///   5. 结束 Setup review → 目的地恢复 Overview，Advanced 状态不被改写
///
/// 预期结果：
///   - 正断言：ASR 和 Spoken Replies 都能成为明确的 Setup 目的地
///   - 负断言：展开 Setup 不得连带展开 Advanced
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
