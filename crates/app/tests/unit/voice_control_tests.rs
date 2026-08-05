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

/// VC-CAPTION-1：每段新语音字幕默认显示全文，用户仍可临时收起当前字幕。
///
/// 数据构造（含关键数值的推导过程）：
///   caption entry paths = 4（初始状态 + 单次录音 + 仅转写结果 + runtime 重置）
///   manual collapses    = 3（每条可见字幕路径前各收起一次）
///   expected defaults   = 4 次展开，证明默认策略覆盖全部新字幕入口
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建默认 Voice 状态 → 尚无字幕，但下一段字幕的展示策略应为全文。
///   2. 用户手动收起 → 当前展示策略进入折叠态；开始新录音 → 恢复全文。
///   3. 用户再次收起；写入仅转写结果 → 新结果恢复全文。
///   4. 用户第三次收起；重置 Voice runtime → 下一段字幕恢复全文默认。
///
/// 预期结果：
///   - 正断言：默认状态、单次录音、仅转写结果和 runtime 重置后都处于全文态。
///   - 负断言：手动收起后不得仍处于全文态，证明「收起」按钮继续有效。
#[test]
fn test_new_voice_caption_defaults_to_full_after_each_entry_path() {
    let mut state = VoiceControlState::default();

    assert!(
        state.caption_expanded(),
        "默认 Voice 状态必须把下一段识别字幕设为全文展示"
    );

    state.toggle_caption_expanded();
    assert!(
        !state.caption_expanded(),
        "用户手动收起当前字幕后不得继续处于全文态"
    );
    assert!(
        state.begin_recording(VoiceTrigger::Shortcut),
        "从默认状态开始新的单次录音必须成功"
    );
    assert!(
        state.caption_expanded(),
        "新的单次录音必须恢复全文字幕默认，而不是继承上一段的收起状态"
    );

    state.toggle_caption_expanded();
    assert!(
        !state.caption_expanded(),
        "写入仅转写结果前的手动收起必须真实生效"
    );
    state.set_transcript_only("完整识别结果", "no executable command");
    assert!(state.caption_expanded(), "新的仅转写结果必须默认显示全文");

    state.toggle_caption_expanded();
    assert!(
        !state.caption_expanded(),
        "runtime 重置前的手动收起必须真实生效"
    );
    state.stop_for_model_switch();
    assert!(
        state.caption_expanded(),
        "Voice runtime 重置后必须为下一段字幕恢复全文默认"
    );
}

/// VC-ERROR-1：关闭单次语音错误后恢复 Idle，并清除麦克风设置恢复入口。
///
/// 数据构造（含关键数值的推导过程）：
///   voice errors     = 1（MicrophoneDenied）
///   dismiss actions = 1（用户点击错误态 ✕）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `set_speech_error(MicrophoneDenied)` → Error，并开放系统设置恢复入口。
///   2. `dismiss_error()` → 清除当前错误反馈。
///   3. 分别读取 phase、visual 与恢复入口。
///
/// 预期结果：
///   - 正断言：关闭后 phase 与 visual 都恢复 Idle。
///   - 负断言：关闭后不得残留 Error，也不得继续开放旧权限错误的恢复入口。
#[test]
fn test_dismiss_voice_error_restores_idle_and_clears_recovery_action() {
    let mut state = VoiceControlState::default();
    state.set_speech_error(&voice_sherpa::SpeechError::MicrophoneDenied);

    state.dismiss_error();

    assert_eq!(
        state.phase(),
        &VoicePhase::Idle,
        "关闭语音错误后 phase 必须恢复 Idle"
    );
    assert_eq!(
        state.visual(),
        VoiceVisual::Idle,
        "关闭语音错误后不得继续渲染 Error 视觉"
    );
    assert!(
        !state.microphone_settings_recovery_available(),
        "关闭语音错误后不得残留旧麦克风权限恢复入口"
    );
    assert_ne!(
        state.visual(),
        VoiceVisual::Error,
        "关闭动作完成后 Error 视觉必须消失"
    );
}

/// VC-1: 语音快捷键按当前配置匹配，普通 Cmd+V / Cmd+1 不应误匹配。
///
/// 数据构造（含关键数值的推导过程）：
///   configured = Cmd+E（默认）与 Cmd+Shift+E（用户改键）
///   checked shortcuts = 4（Cmd+E + Cmd+Shift+E + Cmd+V + Cmd+1）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以默认 Cmd+E 配置匹配 Cmd+E / Cmd+V / Cmd+1
///   2. 以 Cmd+Shift+E 配置匹配对应按键
///
/// 预期结果：
///   - 正断言：只有与配置一致的按键匹配
///   - 负断言：Cmd+V、Cmd+1，以及未配置时都不匹配
#[test]
fn test_voice_hold_shortcut_matches_configured_keystroke_only() {
    let default_voice = key("e", true, false);
    let reassigned = key("e", true, true);
    let paste = key("v", true, false);
    let other = key("1", true, false);

    assert!(
        matches_configured_voice_shortcut(&default_voice, Some(&default_voice)),
        "默认 Cmd+E 应匹配语音快捷键"
    );
    assert!(
        !matches_configured_voice_shortcut(&paste, Some(&default_voice)),
        "Cmd+V 是粘贴，不应触发语音"
    );
    assert!(
        !matches_configured_voice_shortcut(&other, Some(&default_voice)),
        "Cmd+1 不是语音快捷键"
    );
    assert!(
        matches_configured_voice_shortcut(&reassigned, Some(&reassigned)),
        "用户改成 Cmd+Shift+E 后应按新配置匹配"
    );
    assert!(
        !matches_configured_voice_shortcut(&default_voice, Some(&reassigned)),
        "改键后旧的 Cmd+E 不应继续触发语音"
    );
    assert!(
        !matches_configured_voice_shortcut(&default_voice, None),
        "未配置语音快捷键时不得匹配任何按键"
    );
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

/// VC-3: 配置所需修饰键松开时，快捷键录音应进入释放路径。
///
/// 数据构造（含关键数值的推导过程）：
///   configured = Cmd+E 与 Cmd+Shift+E
///   modifier states = 3（Cmd down + Cmd up + Shift changed）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 Cmd+E 检查 Cmd 仍按住 / 已松开 / 仅 Shift 变化
///   2. 以 Cmd+Shift+E 检查 Shift 松开
///
/// 预期结果：
///   - 正断言：配置所需修饰键松开时结束录音
///   - 负断言：所需修饰键仍按住时不结束录音
#[test]
fn test_voice_hold_modifier_release_matches_required_modifiers() {
    let cmd_e = key("e", true, false);
    let cmd_shift_e = key("e", true, true);
    let cmd_down = modifiers(true, false);
    let cmd_up = modifiers(false, false);
    let shift_changed = modifiers(true, true);

    assert!(
        !is_voice_hold_modifier_release(&cmd_down, Some(&cmd_e)),
        "Cmd 仍按住时不应结束快捷键录音"
    );
    assert!(
        is_voice_hold_modifier_release(&cmd_up, Some(&cmd_e)),
        "Cmd 松开时应结束快捷键录音"
    );
    assert!(
        !is_voice_hold_modifier_release(&shift_changed, Some(&cmd_e)),
        "Shift 状态变化但 Cmd 仍按住时不应结束默认 Cmd+E 录音"
    );
    assert!(
        is_voice_hold_modifier_release(&cmd_down, Some(&cmd_shift_e)),
        "Cmd+Shift+E 配置下松开 Shift 应结束录音"
    );
    assert!(
        !is_voice_hold_modifier_release(&cmd_down, None),
        "未配置语音快捷键时修饰键变化不得结束录音"
    );
}
