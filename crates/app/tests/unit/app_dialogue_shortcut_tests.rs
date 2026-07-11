use super::{DialogueControlState, VoiceShortcutKeyDownAction};
use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsValues};
use dialogue::DialoguePhase;
use tempfile::tempdir;
use voice_sherpa::SpeechError;

fn mic_ok() -> Result<(), SpeechError> {
    Ok(())
}

/// DLG-15: 开启 Always Listening 后，Cmd+Shift+V 应进入实时对话而不是旧 one-shot Planner。
///
/// 数据构造（含关键数值的推导过程）：
///   dialogue 初始 Idle；分别传入 always_listening_enabled = true / false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对开启配置计算快捷键 key-down action
///   2. 对关闭配置计算快捷键 key-down action
///
/// 预期结果：
///   - 正断言：开启时应启动 Always Listening shell
///   - 正断言：关闭时应开始 one-shot 长按录音
///   - 负断言：开启时不得继续进入不会播报回复的旧 Planner 路径
#[test]
fn test_voice_shortcut_routes_to_dialogue_when_always_listening_is_enabled() {
    let state = DialogueControlState::default();

    assert_eq!(
        state.voice_shortcut_key_down_action(true),
        VoiceShortcutKeyDownAction::StartAlwaysListeningShell,
        "Always Listening 开启时 Cmd+Shift+V 应启动实时对话 shell"
    );
    assert_eq!(
        state.voice_shortcut_key_down_action(false),
        VoiceShortcutKeyDownAction::StartOneShotRecording,
        "Always Listening 关闭时 Cmd+Shift+V 应保留 one-shot 长按录音"
    );
    assert_ne!(
        state.voice_shortcut_key_down_action(true),
        VoiceShortcutKeyDownAction::StartOneShotRecording,
        "Always Listening 开启时不得误入旧 one-shot Planner"
    );
}

/// DLG-16: 对话轮次失败后应恢复 Listening，下一句可清除旧错误并重新进入 Thinking。
///
/// 数据构造（含关键数值的推导过程）：
///   session 经 Listening → final「你好」→ Thinking；注入 Ollama request failed
///
/// 执行过程（逐步说明系统如何处理）：
///   1. recover_reply_failure 记录错误并恢复 Listening
///   2. on_final_transcript("再试一次") 发起下一轮
///
/// 预期结果：
///   - 正断言：失败后 phase == Listening 且错误可见
///   - 正断言：下一句进入 Thinking 并清除旧错误
///   - 负断言：不得永久卡在 Thinking 导致后续识别无回复
#[test]
fn test_dialogue_turn_failure_recovers_for_next_utterance() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let saved = SavedVoiceSettings {
            realtime_enabled: Some(true),
            paraformer_model_dir: Some(dir.path().to_path_buf()),
            ..SavedVoiceSettings::default()
        };
        let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
        let mut state = DialogueControlState::default();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");
        state
            .on_final_transcript("你好")
            .expect("第一句 final 应进入 Thinking");

        state.recover_reply_failure("Turn error: Ollama request failed");

        assert_eq!(
            state.phase(),
            DialoguePhase::Listening,
            "轮次失败后应恢复 Listening，不能卡在 Thinking"
        );
        assert_eq!(
            state.dialogue_voice_label().as_deref(),
            Some("Turn error: Ollama request failed"),
            "恢复 Listening 时仍应向用户展示本轮错误"
        );

        state
            .on_final_transcript("再试一次")
            .expect("恢复后下一句应能重新进入 Thinking");

        assert_eq!(
            state.phase(),
            DialoguePhase::Thinking,
            "下一句 final 应重新进入 Thinking"
        );
        assert_eq!(
            state.dialogue_voice_label().as_deref(),
            Some("Thinking..."),
            "新一轮开始后应清除旧错误并显示 Thinking"
        );
    });
}

/// DLG-17: Speaking 时再次按 Cmd+Shift+V 应作为确认打断，停止回复并回 Listening。
///
/// 数据构造：
///   Always Listening session 经 Listening → Thinking → Speaking，使用 Recording sink。
///
/// 执行过程：
///   1. 读取 voice_shortcut_key_down_action(true)；
///   2. 执行 interrupt_assistant_reply_for_user_action。
///
/// 预期结果：
///   - 正断言：快捷键 action 为 InterruptAssistantReply；
///   - 正断言：sink 被 stop 且 phase 回 Listening；
///   - 负断言：不得因 dialogue active 而继续返回 Ignore。
#[test]
fn test_voice_shortcut_interrupts_assistant_reply_during_speaking() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let saved = SavedVoiceSettings {
            realtime_enabled: Some(true),
            paraformer_model_dir: Some(dir.path().to_path_buf()),
            ..SavedVoiceSettings::default()
        };
        let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
        let mut state = DialogueControlState::default();
        state.prepare_reply_test_fixtures();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");
        state
            .on_final_transcript("你好")
            .expect("final 应进入 Thinking");
        state.run_voice_turn_for_tests().expect("测试 turn 应完成");
        state
            .on_reply_ready_for_tests()
            .expect("测试回复应进入 Speaking");
        let sink = state.assistant_audio_sink_for_tests();

        let action = state.voice_shortcut_key_down_action(true);
        let interrupted = state.interrupt_assistant_reply_for_user_action();

        assert_eq!(
            action,
            VoiceShortcutKeyDownAction::InterruptAssistantReply,
            "Speaking 时 Cmd+Shift+V 应映射为确认打断"
        );
        assert!(interrupted, "确认打断应返回 true");
        assert!(sink.stopped(), "确认打断应停止 assistant sink");
        assert_eq!(
            state.phase(),
            DialoguePhase::Listening,
            "确认打断后应回 Listening"
        );
        assert_ne!(
            action,
            VoiceShortcutKeyDownAction::Ignore,
            "Speaking 时不得忽略用户的显式打断快捷键"
        );
    });
}
