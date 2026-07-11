use super::{DialogueControlState, DialogueStartError, VoiceBarMouseDownAction};
use crate::app::voice::control::VoiceVisual;
use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsValues};
use dialogue::DialoguePhase;
use std::sync::Mutex;
use tempfile::tempdir;
use voice_sherpa::SpeechError;

static DIALOGUE_SHELL_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn with_dialogue_shell_test_guard(assertions: impl FnOnce()) {
    let _guard = DIALOGUE_SHELL_TEST_LOCK
        .lock()
        .expect("dialogue shell test lock should not be poisoned");
    DialogueControlState::reset_test_seams_for_tests();
    assertions();
    DialogueControlState::reset_test_seams_for_tests();
}

pub(super) fn mic_ok() -> Result<(), SpeechError> {
    Ok(())
}

pub(super) fn always_listening_values(
    model_dir: Option<std::path::PathBuf>,
) -> VoiceSettingsValues {
    let saved = SavedVoiceSettings {
        realtime_enabled: Some(true),
        paraformer_model_dir: model_dir,
        ..SavedVoiceSettings::default()
    };
    VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default())
}

/// DLG-1: Always Listening 启动校验不要求 OpenAI API key。
///
/// 数据构造（含关键数值的推导过程）：
///   model_dir = tempfile 创建的现存目录（满足 SherpaProvider::new 存在性检查）
///   realtime_enabled = true（表示用户已打开 Always Listening 开关）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造仅含本地 sherpa 配置的 VoiceSettingsValues
///   2. 调用 DialogueControlState::validate_start_provider
///
/// 预期结果：
///   - 正断言：validate_start_provider 返回 Ok，本地 provider 可解析
///   - 负断言：校验结果不应因缺少 OpenAI 设置而失败
#[test]
fn test_dialogue_start_does_not_require_openai_api_key() {
    let dir = tempdir().expect("temp model dir should be creatable");
    let values = always_listening_values(Some(dir.path().to_path_buf()));

    DialogueControlState::validate_start_provider(&values)
        .expect("存在本地 model dir 时启动校验应成功，且不依赖 OpenAI 设置");
}

/// DLG-2: 停止 Always Listening 后 phase 回到 Idle，但 models_resident 仍为 true。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 phase = Idle
///   on_listening_started 后 phase = Listening 且 models_resident = true
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认 DialogueControlState
///   2. 通过 begin_listening_sync_for_tests 进入 Listening
///   3. 调用 stop_shell
///   4. 读取 phase 与 models_resident
///
/// 预期结果：
///   - 正断言：stop 后 phase == Idle
///   - 正断言：stop 后 models_resident 仍为 true
///   - 负断言：stop 不应把 models_resident 清回 false
#[test]
fn test_dialogue_stop_returns_idle_but_models_remain_resident() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应成功进入 Listening");

        assert_eq!(
            state.phase(),
            DialoguePhase::Listening,
            "启动后应处于 Listening phase"
        );
        assert!(
            state.models_resident(),
            "首次 Listening 启动后 models_resident 应为 true"
        );

        state.stop_shell();

        assert_eq!(
            state.phase(),
            DialoguePhase::Idle,
            "stop_shell 后 phase 应回到 Idle"
        );
        assert!(state.models_resident(), "关闭 Listening 不应卸载已驻留模型");
    });
}

/// DLG-3: 麦克风权限被拒绝时，启动应干净失败并在底栏暴露错误。
///
/// 数据构造（含关键数值的推导过程）：
///   mic seam = 注入闭包，固定返回 SpeechError::MicrophoneDenied
///   model_dir = tempfile 现存目录（排除 model 缺失干扰）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 mic permission seam 返回 MicrophoneDenied
///   2. 调用 begin_listening_sync_for_tests
///   3. 读取 phase、error_message、effective_voice_label/visual
///
/// 预期结果：
///   - 正断言：返回 MicrophoneDenied 错误
///   - 正断言：phase 保持 Idle
///   - 正断言：底栏 label/visual 映射到麦克风错误
///   - 负断言：不应进入 Listening 或 Always Listening label
#[test]
fn test_dialogue_start_fails_cleanly_when_mic_permission_denied() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();

        DialogueControlState::set_mic_permission_check_for_tests(Some(|| {
            Err(SpeechError::MicrophoneDenied)
        }));

        let error = state
            .begin_listening_sync_for_tests(&values)
            .expect_err("麦克风被拒绝时启动应失败");

        assert!(
            matches!(
                error,
                DialogueStartError::Speech(SpeechError::MicrophoneDenied)
            ),
            "启动失败原因应是 MicrophoneDenied，实际: {error}"
        );
        assert_eq!(
            state.phase(),
            DialoguePhase::Idle,
            "权限拒绝后 phase 应保持 Idle"
        );
        assert!(
            state
                .effective_voice_label("Voice")
                .to_ascii_lowercase()
                .contains("microphone"),
            "底栏应显示麦克风相关错误，实际: {}",
            state.effective_voice_label("Voice")
        );
        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Error,
            "麦克风拒绝应映射 Error 视觉"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "权限拒绝时不应显示 Always Listening"
        );
    });
}

/// DLG-4: 默认 dialogue shell 不覆盖 one-shot voice 底栏。
///
/// 数据构造（含关键数值的推导过程）：
///   state = default → phase Idle；fallback label = Voice；fallback visual = Idle
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 DialogueControlState
///   2. 读取 dialogue_voice_label / effective label / effective visual
///
/// 预期结果：
///   - 正断言：Idle 时 effective label 为 Voice，visual 为 Idle
///   - 负断言：Idle 时不应显示 Always Listening 或错误文案
#[test]
fn test_dialogue_default_maps_bottom_bar_to_voice() {
    let state = DialogueControlState::default();

    assert!(
        state.dialogue_voice_label().is_none(),
        "Idle 状态不应覆盖 one-shot voice label"
    );
    assert_eq!(
        state.effective_voice_label("Voice"),
        "Voice",
        "默认 dialogue shell 应让底栏继续显示 one-shot Voice"
    );
    assert_eq!(
        state.effective_voice_visual(VoiceVisual::Idle),
        VoiceVisual::Idle,
        "默认 dialogue shell 应保留 one-shot Idle 视觉"
    );
}

/// DLG-5: Listening phase 映射 Always Listening label 与 Recording visual。
///
/// 数据构造（含关键数值的推导过程）：
///   model_dir = tempfile 目录
///   启动后 phase = Listening
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 begin_listening_sync_for_tests 进入 Listening
///   2. 读取 effective_voice_label / effective_voice_visual
///
/// 预期结果：
///   - 正断言：label 为 Always Listening
///   - 正断言：visual 为 Recording
#[test]
fn test_dialogue_listening_maps_bottom_label_to_always_listening() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "Listening phase 底栏应显示 Always Listening"
        );
        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Recording,
            "Listening 状态应使用 Recording 视觉"
        );
    });
}

/// DLG-6: Always Listening 开关开启且未 active 时，底栏 mouse down 应启动 shell。
///
/// 数据构造（含关键数值的推导过程）：
///   always_listening_enabled = true
///   phase = Idle
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认 state，realtime_enabled = true
///   2. 调用 voice_bar_mouse_down_action
///
/// 预期结果：
///   - 正断言：返回 StartAlwaysListeningShell
#[test]
fn test_dialogue_voice_bar_mouse_down_starts_shell_when_enabled() {
    let state = DialogueControlState::default();

    assert_eq!(
        state.voice_bar_mouse_down_action(true),
        VoiceBarMouseDownAction::StartAlwaysListeningShell,
        "Always Listening 开启且 Idle 时底栏应启动 shell"
    );
    assert_eq!(
        state.voice_bar_mouse_down_action(false),
        VoiceBarMouseDownAction::StartOneShotRecording,
        "Always Listening 关闭时应保留 one-shot 录音入口"
    );
}

/// DLG-8: final transcript 应把 session 推进 Thinking 并保存 pending_transcript。
///
/// 执行过程：
///   1. begin_listening_sync_for_tests 进入 Listening
///   2. on_final_transcript("你好")
///
/// 预期结果：
///   - 正断言：phase == Thinking
///   - 正断言：pending_transcript == Some("你好")
#[test]
fn test_dialogue_final_transcript_moves_to_thinking() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");

        state
            .on_final_transcript("你好")
            .expect("Listening 下 final transcript 应成功");

        assert_eq!(
            state.phase(),
            DialoguePhase::Thinking,
            "final transcript 后应进入 Thinking"
        );
        assert_eq!(
            state.pending_transcript(),
            Some("你好"),
            "Thinking 前应保存 pending_transcript"
        );
        assert_eq!(
            state.effective_voice_label("Voice"),
            "Thinking...",
            "Thinking phase 底栏应显示 Thinking..."
        );
    });
}

/// DLG-10: final transcript 后 Mock echo turn 应保存回复文本并可通过 TTS 进入 Speaking。
///
/// 执行过程：
///   1. 进入 Listening → Thinking
///   2. run_voice_turn_for_tests（默认 echo Mock）
///   3. on_reply_ready_for_tests
///
/// 预期结果：
///   - 正断言：last_reply_text ==「收到：你好」
///   - 正断言：phase == Speaking
///   - 负断言：card task cancel hook 未被调用
#[test]
fn test_dialogue_echo_turn_reply_text_reaches_speaking() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();
        state.prepare_reply_test_fixtures();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");
        state
            .on_final_transcript("你好")
            .expect("final transcript 应进入 Thinking");
        state
            .run_voice_turn_for_tests()
            .expect("echo Mock turn 应成功");

        assert_eq!(
            state.last_reply_text_for_tests(),
            Some("收到：你好"),
            "默认 echo Mock 应保存回复文本"
        );
        assert!(
            state.queued_template_reports_for_tests().is_empty(),
            "纯文本 echo turn 不应排队模板报告"
        );

        state
            .on_reply_ready_for_tests()
            .expect("reply TTS 应进入 Speaking");
        assert_eq!(
            state.phase(),
            DialoguePhase::Speaking,
            "TTS 合成后应进入 Speaking"
        );
        assert!(
            !state.card_tasks_were_cancelled_for_tests(),
            "echo turn 不应取消 card tasks"
        );
    });
}

/// DLG-14: Speaking 时原始麦克风 SpeechStart 未分类，不能直接截断 assistant TTS。
///
/// 执行过程：
///   1. 进入 Listening → Thinking → Speaking
///   2. 注入 pipeline 原始 SpeechStart（可能是扬声器回声）
///
/// 预期结果：
///   - 正断言：phase 仍为 Speaking
///   - 负断言：recording sink 不得 stop
///   - 负断言：card task cancel hook 未被调用
#[test]
fn test_dialogue_unclassified_speech_does_not_truncate_assistant_reply() {
    with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values = always_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();
        state.prepare_reply_test_fixtures();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        state
            .begin_listening_sync_for_tests(&values)
            .expect("测试启动应进入 Listening");
        state
            .on_final_transcript("你好")
            .expect("final transcript 应进入 Thinking");
        state.run_voice_turn_for_tests().expect("voice turn 应完成");
        state
            .on_reply_ready_for_tests()
            .expect("reply TTS 应进入 Speaking");

        let sink = state.assistant_audio_sink_for_tests();
        assert!(!sink.stopped(), "前置：Speaking 播放尚未被 stop");
        assert!(
            !state.card_tasks_were_cancelled_for_tests(),
            "前置：card tasks 尚未被取消"
        );

        assert!(
            !state.on_unclassified_speech_start_for_tests(),
            "未分类 SpeechStart 可能来自 assistant 扬声器回声，不应直接处理为 barge-in"
        );

        assert_eq!(
            state.phase(),
            DialoguePhase::Speaking,
            "忽略 assistant 回声候选后应继续 Speaking"
        );
        assert!(!sink.stopped(), "assistant 回声不得停止正在播放的完整回复");
        assert!(
            !state.card_tasks_were_cancelled_for_tests(),
            "assistant 回声不应取消 card tasks"
        );
        assert!(
            state.models_resident(),
            "忽略 assistant 回声后 models_resident 应保持 true"
        );
    });
}

///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve SavedVoiceSettings
///   2. 读取 realtime_enabled()
///
/// 预期结果：
///   - 正断言：values.realtime_enabled() == true
#[test]
fn test_dialogue_settings_still_resolve_always_listening_toggle() {
    let saved = SavedVoiceSettings {
        realtime_enabled: Some(true),
        ..SavedVoiceSettings::default()
    };
    let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());

    assert!(
        values.realtime_enabled(),
        "Settings 仍应通过 realtime_enabled 字段表示 Always Listening 开关"
    );
}

/// DLG-15：最终语音识别文本必须以 info 日志输出，便于默认日志级别直接观察。
///
/// 数据构造：
///   读取生产 `dialogue/chat.rs`，截取 `FinalTranscript` 分支到状态更新前的日志块；
///   默认日志过滤器为 info，因此 debug 不会出现在普通运行日志中。
///
/// 执行过程：
///   1. 定位 `ListeningPipelineEvent::FinalTranscript(text)` 分支；
///   2. 截取紧随其后的日志语句；
///   3. 检查日志级别和 transcript 字段。
///
/// 预期结果：
///   - 正断言：日志块使用 `tracing::info!` 并输出 `%text`；
///   - 负断言：不得继续使用只在 debug 级别可见的日志。
#[test]
fn test_final_speech_transcript_is_logged_at_info_with_text() {
    let source = include_str!("../../src/app/dialogue/chat.rs");
    let branch_start = source
        .find("ListeningPipelineEvent::FinalTranscript(text) =>")
        .expect("生产 pipeline event pump 应处理 FinalTranscript");
    let update_start = source[branch_start..]
        .find("let should_notify")
        .map(|offset| branch_start + offset)
        .expect("FinalTranscript 日志后应继续更新 dialogue 状态");
    let log_block = &source[branch_start..update_start];

    assert!(
        log_block.contains("tracing::info!("),
        "final transcript 应在默认 info 级别可见；日志块：{log_block:?}"
    );
    assert!(
        log_block.contains("transcript = %text"),
        "info 日志必须打印实际识别文本，而不只是字符数；日志块：{log_block:?}"
    );
    assert!(
        !log_block.contains("tracing::debug!("),
        "final transcript 不得继续只写入 debug 日志；日志块：{log_block:?}"
    );
}
