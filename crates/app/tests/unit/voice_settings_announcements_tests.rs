use super::super::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};

/// Agent 任务播报设置默认开启并可独立切换。
///
/// 数据构造（含关键数值的推导过程）：
///   saved.agent_task_announcements_enabled = None → 产品默认 true
///   toggle count = 1 → true 反转为 false
///   ordinary trace default = false，用于证明两个开关相互独立
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用空 saved/env 构造 VoiceSettingsState。
///   2. 读取 Agent 任务播报默认值。
///   3. 切换一次 Agent 任务播报开关并再次读取。
///
/// 预期结果：
///   - 正断言：首次安装/升级缺少保存值时播报默认开启。
///   - 负断言：切换一次后播报关闭。
///   - 负断言：切换播报不能同时打开 trace。
#[test]
fn test_agent_task_announcements_default_on_and_toggle_independently() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    assert!(
        state.values().agent_task_announcements_enabled(),
        "缺少保存值时 Agent 任务播报必须默认开启"
    );

    state.toggle_agent_task_announcements();

    assert!(
        !state.values().agent_task_announcements_enabled(),
        "默认开启的 Agent 任务播报切换一次后必须关闭"
    );
    assert!(
        !state.values().trace_enabled(),
        "切换 Agent 任务播报不应连带打开独立的诊断 trace 设置"
    );
}

/// 旧「窗口内缘视觉」值仍作为光晕迁移来源读取，默认开启，且与 Agent 音频播报相互独立。
///
/// 数据构造：saved.voice_window_inset_enabled = None → 产品默认 true；Agent 播报默认 true。
/// 执行过程：用空 saved/env 构造 VoiceSettingsState，读取两个设置。
/// 预期结果：两项都默认开启（开关本身已移到 设置 → 特效，不再在语音页切换）。
#[test]
fn test_voice_window_inset_default_on_and_independent_from_audio_announcements() {
    let state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    assert!(
        state.values().voice_window_inset_enabled(),
        "缺少保存值时语音驱动窗口内缘视觉必须默认开启"
    );
    assert!(
        state.values().agent_task_announcements_enabled(),
        "视觉设置测试的 Agent 音频播报基线必须保持默认开启"
    );
}
