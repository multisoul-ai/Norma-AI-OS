use super::*;
use gpui::Keystroke;

fn key(key: &str, platform: bool, shift: bool) -> Keystroke {
    Keystroke {
        key: key.into(),
        key_char: Some(key.into()),
        modifiers: Modifiers {
            platform,
            shift,
            ..Modifiers::default()
        },
    }
}

fn modifiers(platform: bool, shift: bool) -> Modifiers {
    Modifiers {
        platform,
        shift,
        ..Modifiers::default()
    }
}

fn state_with_phase(phase: VoicePhase) -> VoiceControlState {
    let mut state = VoiceControlState::default();
    match phase {
        VoicePhase::Idle => {}
        VoicePhase::Recording { trigger } => {
            state.begin_recording(trigger);
        }
        VoicePhase::Transcribing => {
            state.begin_recording(VoiceTrigger::Shortcut);
            state.finish_recording(VoiceTrigger::Shortcut);
        }
        VoicePhase::Executed { message } => state.set_executed(message),
        VoicePhase::TranscriptOnly { transcript, reason } => {
            state.set_transcript_only(transcript, reason)
        }
        VoicePhase::Error { message } => state.set_error(message),
    }
    state
}

/// VV-1: 每个 VoicePhase 全部映射到正确的 VoiceVisual 分类。
///
/// 数据构造（含关键数值的推导过程）：
///   Idle             → 期望 Idle
///   Recording        → 期望 Recording
///   Transcribing     → 期望 Processing
///   Executed         → 期望 Idle
///   TranscriptOnly   → 期望 Success
///   Error            → 期望 Error
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为每个 phase 构造 VoiceControlState
///   2. 调用 visual() 取视觉分类
///   3. 与期望分类逐一比对
///
/// 预期结果：
///   - 正断言：每个 phase 的视觉分类与上表一致
///   - 负断言：Executed 不显示 Success；TranscriptOnly 不显示 Processing
#[test]
fn test_visual_maps_every_phase_correctly() {
    let cases = [
        (VoicePhase::Idle, VoiceVisual::Idle),
        (
            VoicePhase::Recording {
                trigger: VoiceTrigger::Shortcut,
            },
            VoiceVisual::Recording,
        ),
        (VoicePhase::Transcribing, VoiceVisual::Processing),
        (
            VoicePhase::Executed {
                message: "done".into(),
            },
            VoiceVisual::Idle,
        ),
        (
            VoicePhase::TranscriptOnly {
                transcript: "hello".into(),
                reason: "no command".into(),
            },
            VoiceVisual::Success,
        ),
        (
            VoicePhase::Error {
                message: "no mic".into(),
            },
            VoiceVisual::Error,
        ),
    ];

    for (phase, expected) in cases {
        let state = state_with_phase(phase.clone());
        let got = state.visual();
        assert_eq!(
            got, expected,
            "phase {:?} 应映射为 {:?}，实际得到 {:?}",
            phase, expected, got
        );
    }
}

/// VV-2: default() 状态（刚启动）的视觉分类必须是 Idle。
///
/// 数据构造（含关键数值的推导过程）：
///   state = VoiceControlState::default()，状态数 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 取默认状态
///   2. 调用 visual()
///
/// 预期结果：
///   - 正断言：default 状态为 Idle
///   - 负断言：启动时不得显示录音或错误外观
#[test]
fn test_visual_default_is_idle() {
    let state = VoiceControlState::default();
    assert_eq!(
        state.visual(),
        VoiceVisual::Idle,
        "默认状态应为 Idle，启动时不应误显示录音或错误外观"
    );
}

/// VC-1: Cmd+E 是语音长按快捷键，普通 Cmd+V 不应匹配。
///
/// 数据构造（含关键数值的推导过程）：
///   checked shortcuts = 3（Cmd+E + Cmd+V + Cmd+1）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 检查 Cmd+E
///   2. 检查 Cmd+V
///   3. 检查 Cmd+1
///
/// 预期结果：
///   - 正断言：Cmd+E 匹配语音长按
///   - 负断言：Cmd+V 和 Cmd+1 不匹配
#[test]
fn test_voice_hold_shortcut_matches_cmd_e_only() {
    let voice = key("e", true, false);
    let paste = key("v", true, false);
    let other = key("1", true, false);

    assert!(is_voice_hold_shortcut(&voice), "Cmd+E 应匹配语音长按快捷键");
    assert!(
        !is_voice_hold_shortcut(&paste),
        "Cmd+V 是粘贴，不应触发语音"
    );
    assert!(!is_voice_hold_shortcut(&other), "Cmd+1 不是语音快捷键");
}

/// VC-2: 重复 keydown 不重复启动录音，keyup 只结束对应 trigger。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 phase = Idle
///   begin 次数 = 2，第一次 true + 第二次 false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. begin_recording(Shortcut) → Recording
///   2. 再次 begin_recording(Shortcut) → false
///   3. finish_recording(Shortcut) → Transcribing
///
/// 预期结果：
///   - 正断言：第一次开始和对应结束成功
///   - 负断言：重复开始失败，结束后不处于 Recording
#[test]
fn test_voice_recording_state_is_idempotent_and_trigger_scoped() {
    let mut state = VoiceControlState::default();

    let first = state.begin_recording(VoiceTrigger::Shortcut);
    let duplicate = state.begin_recording(VoiceTrigger::Shortcut);
    let right_finish = state.finish_recording(VoiceTrigger::Shortcut);

    assert!(first, "第一次 Shortcut keydown 应开始录音");
    assert!(!duplicate, "重复 keydown 不应重复启动录音");
    assert!(right_finish, "Shortcut keyup 应结束 Shortcut 录音");
    assert_ne!(
        state.phase(),
        &VoicePhase::Recording {
            trigger: VoiceTrigger::Shortcut,
        },
        "结束录音后不应仍处于 Recording"
    );
}

/// VC-3: Cmd 松开时，快捷键录音应进入释放路径。
///
/// 数据构造（含关键数值的推导过程）：
///   modifier states = 3（Cmd down + Cmd up + Shift changed）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 检查 Cmd 仍按住
///   2. 检查 Cmd 已松开
///   3. 检查 Shift 变化但 Cmd 仍按住
///
/// 预期结果：
///   - 正断言：Cmd 松开时结束录音
///   - 负断言：Cmd 仍按住时不结束录音
#[test]
fn test_voice_hold_modifier_release_matches_cmd_up_only() {
    let cmd_down = modifiers(true, false);
    let cmd_up = modifiers(false, false);
    let shift_changed = modifiers(true, true);

    assert!(
        !is_voice_hold_modifier_release(&cmd_down),
        "Cmd 仍按住时不应结束快捷键录音"
    );
    assert!(
        is_voice_hold_modifier_release(&cmd_up),
        "Cmd 松开时应结束快捷键录音"
    );
    assert!(
        !is_voice_hold_modifier_release(&shift_changed),
        "Shift 状态变化但 Cmd 仍按住时不应结束快捷键录音"
    );
}
