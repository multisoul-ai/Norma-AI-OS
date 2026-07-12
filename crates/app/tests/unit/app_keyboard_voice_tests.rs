use super::{dictation_shortcut_action, DictationShortcutAction};

/// KB-VOICE-1：Toggle 听写第二次按键停止，Hold 的重复 keydown 不承担停止语义。
///
/// 数据构造（含关键数值的推导过程）：
///   trigger variants = 2（Hold + Toggle）
///   recording states = 2（false + true）
///   checked combinations = 3（Toggle/idle、Toggle/recording、Hold/recording）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Toggle + idle → Start
///   2. Toggle + recording → Stop
///   3. Hold + recording → Start（重复启动会由 VoiceControlState 幂等拒绝）
///
/// 预期结果：
///   - 正断言：Toggle 的两次按键形成 Start/Stop
///   - 负断言：Hold 不得被错误映射成第二次按键停止
#[test]
fn test_dictation_toggle_uses_second_shortcut_press_to_stop() {
    let toggle_idle =
        dictation_shortcut_action(crate::settings::voice::DictationTrigger::Toggle, false);
    let toggle_recording =
        dictation_shortcut_action(crate::settings::voice::DictationTrigger::Toggle, true);
    let hold_recording =
        dictation_shortcut_action(crate::settings::voice::DictationTrigger::Hold, true);

    assert_eq!(
        toggle_idle,
        DictationShortcutAction::Start,
        "Toggle 空闲时第一次快捷键应开始录音"
    );
    assert_eq!(
        toggle_recording,
        DictationShortcutAction::Stop,
        "Toggle 录音中第二次快捷键应停止录音"
    );
    assert_eq!(
        hold_recording,
        DictationShortcutAction::Start,
        "Hold 模式应由 keyup 停止，不应把重复 keydown 当成 Toggle"
    );
    assert_ne!(
        hold_recording,
        DictationShortcutAction::Stop,
        "Hold 模式不得采用第二次按键停止语义"
    );
}
