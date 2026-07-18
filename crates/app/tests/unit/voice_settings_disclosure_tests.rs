use super::*;

/// VS-D1：Setup 模型目录与 Ready Advanced 必须是两个互不影响的展开状态。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Setup 展开状态 = false
///   初始 Advanced 展开状态 = false
///   两次操作分别只切换其中 1 个布尔状态
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 VoiceSettingsState → 两个区域都折叠
///   2. 展开 Setup 模型目录 → Advanced 仍折叠
///   3. 展开 Ready Advanced → Setup 模型目录保持展开
///   4. 结束 Setup review → Setup 模型目录恢复折叠，Advanced 状态不被改写
///
/// 预期结果：
///   - 正断言：每个区域都能独立展开
///   - 负断言：展开 Setup 不得连带展开 Advanced
///   - 负断言：离开 Setup 后不得残留 Setup 模型目录展开状态
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
