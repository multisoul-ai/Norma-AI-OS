use super::{DialogueControlState, VoiceShortcutKeyDownAction};
use dialogue::DialoguePhase;
use tempfile::tempdir;
use voice_sherpa::SpeechError;

fn mic_ok() -> Result<(), SpeechError> {
    Ok(())
}

/// DLG-SETTINGS-1：持续聆听必须由新交互模式表达，不再依赖旧布尔开关。
///
/// 数据构造（含关键数值的推导过程）：
///   saved mode = ContinuousListening
///   legacy realtime keys = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve SavedVoiceSettings
///   2. 读取 interaction_mode()
///
/// 预期结果：
///   - 正断言：模式精确为 ContinuousListening
///   - 负断言：不得回退为 SingleVoiceCommand
#[test]
fn test_dialogue_settings_resolve_continuous_listening_mode() {
    use crate::settings::voice::{
        EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode, VoiceSettingsValues,
    };

    let saved = SavedVoiceSettings {
        interaction_mode: Some(VoiceInteractionMode::ContinuousListening),
        ..SavedVoiceSettings::default()
    };
    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());

    assert_eq!(
        values.interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "Settings 应通过交互模式表达持续聆听"
    );
    assert_ne!(
        values.interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "持续聆听设置不得回退为单次语音指令模式"
    );
}

/// DLG-15: 开启 Continuous Listening 后，Cmd+E 应进入实时对话而不是单次语音指令。
///
/// 数据构造（含关键数值的推导过程）：
///   dialogue 初始 Idle；分别传入 continuous_listening_enabled = true / false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对开启配置计算快捷键 key-down action
///   2. 对关闭配置计算快捷键 key-down action
///
/// 预期结果：
///   - 正断言：开启时应启动 Continuous Listening shell
///   - 正断言：关闭时应开始 one-shot 长按录音
///   - 负断言：开启时不得继续进入不会播报回复的旧 Planner 路径
#[test]
fn test_voice_shortcut_routes_to_dialogue_in_continuous_listening_mode() {
    let state = DialogueControlState::default();

    assert_eq!(
        state.voice_shortcut_key_down_action(true),
        VoiceShortcutKeyDownAction::StartContinuousListening,
        "Continuous Listening 开启时 Cmd+E 应启动实时对话 shell"
    );
    assert_eq!(
        state.voice_shortcut_key_down_action(false),
        VoiceShortcutKeyDownAction::StartOneShotRecording,
        "Continuous Listening 关闭时 Cmd+Shift+V 应保留 one-shot 长按录音"
    );
    assert_ne!(
        state.voice_shortcut_key_down_action(true),
        VoiceShortcutKeyDownAction::StartOneShotRecording,
        "Continuous Listening 开启时不得误入旧 one-shot Planner"
    );
}

/// DLG-MODE-1：持续聆听会话运行时再次按 ⌘⇧V 必须请求停止会话。
///
/// 数据构造（含关键数值的推导过程）：
///   interaction mode = ContinuousListening
///   dialogue phase   = Listening（已启动长会话）
///   shortcut presses = 1（第二次显式用户动作）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造有效 Paraformer 安装并进入 Listening
///   2. 调用 voice_shortcut_key_down_action(true)
///
/// 预期结果：
///   - 正断言：返回 StopContinuousListening
///   - 负断言：运行中的会话不得忽略第二次快捷键
#[test]
fn test_continuous_listening_shortcut_stops_an_active_session() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("测试应创建临时模型根");
        let values = super::app_dialogue_shell_tests::continuous_listening_values(Some(
            dir.path().to_path_buf(),
        ));
        let mut state = DialogueControlState::default();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect("有效模型应进入 Listening");

        let action = state.voice_shortcut_key_down_action(true);

        assert_eq!(
            action,
            VoiceShortcutKeyDownAction::StopContinuousListening,
            "持续聆听运行时再次按快捷键应停止长会话"
        );
        assert_ne!(
            action,
            VoiceShortcutKeyDownAction::StartContinuousListening,
            "第二次显式快捷键不得重新启动已运行的持续聆听"
        );
    });
}

/// DLG-16: 对话轮次失败后应恢复 Listening，下一句可清除旧错误并重新进入 Thinking。
///
/// 数据构造（含关键数值的推导过程）：
///   final transcripts = 2（首轮「你好」+ 恢复轮「再试一次」）
///   injected failures = 1（Ollama request failed）
///   recovery cycles   = 1（Thinking → Listening → Thinking）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. recover_reply_failure 记录错误并恢复 Listening
///   2. on_final_transcript("再试一次") 发起下一轮
///
/// 预期结果：
///   - 正断言：失败后 phase == Listening 且错误可见
///   - 正断言：下一句进入 Thinking，顶部 Voice 入口显示「再试一次」
///   - 负断言：下一轮不得继续显示旧错误
///   - 负断言：不得永久卡在 Thinking 导致后续识别无回复
#[test]
fn test_dialogue_turn_failure_recovers_for_next_utterance() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = super::app_dialogue_shell_tests::continuous_listening_values(Some(
            dir.path().to_path_buf(),
        ));
        let mut state = DialogueControlState::default();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
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
        let recovered_label = state.dialogue_voice_label();
        assert_eq!(
            recovered_label.as_deref(),
            Some("再试一次"),
            "新一轮开始后应清除旧错误并显示当前 final transcript"
        );
        assert_ne!(
            recovered_label.as_deref(),
            Some("Turn error: Ollama request failed"),
            "新一轮开始后不得继续显示上一轮错误"
        );
    });
}

/// DLG-ERROR-SHORTCUT-1：持续聆听回合失败后，快捷键先清错并露出存活的 Listening。
///
/// 数据构造（含关键数值的推导过程）：
///   dialogue phase   = Listening（有效长会话）
///   failed turns     = 1（`recover_reply_failure`）
///   shortcut presses = 1（错误出现后的首次 ⌘E）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 Continuous Listening 并提交一句 final transcript。
///   2. 模拟 Agent 回合失败，状态机回到 Listening 并保留错误反馈。
///   3. 计算下一次 ⌘E 的类型安全 action。
///
/// 预期结果：
///   - 正断言：错误态首次快捷键返回 DismissError。
///   - 负断言：不得返回 StopContinuousListening，把存活的录音会话误停。
#[test]
fn test_continuous_listening_error_shortcut_dismisses_feedback_without_stopping() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("测试应创建持续聆听错误恢复的临时模型根");
        let values = super::app_dialogue_shell_tests::continuous_listening_values(Some(
            dir.path().to_path_buf(),
        ));
        let mut state = DialogueControlState::default();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect("有效模型应进入 Listening");
        state
            .on_final_transcript("你好")
            .expect("final transcript 应进入 Thinking");
        state.recover_reply_failure("Turn error: MaxTurnsError");

        let action = state.voice_shortcut_key_down_action(true);

        assert_eq!(
            action,
            VoiceShortcutKeyDownAction::DismissError,
            "持续聆听错误态首次 ⌘E 应清除错误并露出存活会话"
        );
        assert_ne!(
            action,
            VoiceShortcutKeyDownAction::StopContinuousListening,
            "持续聆听错误态首次 ⌘E 不得停止仍在收音的会话"
        );
    });
}

/// DLG-17: Speaking 时再次按 Cmd+Shift+V 应作为确认打断，停止回复并回 Listening。
///
/// 数据构造：
///   Continuous Listening session 经 Listening → Thinking → Speaking，使用 Recording sink。
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
        let values = super::app_dialogue_shell_tests::continuous_listening_values(Some(
            dir.path().to_path_buf(),
        ));
        let mut state = DialogueControlState::default();
        state.prepare_reply_test_fixtures();
        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
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
            VoiceShortcutKeyDownAction::StopContinuousListening,
            "Speaking 时不得把显式打断降级为普通停止"
        );
    });
}
