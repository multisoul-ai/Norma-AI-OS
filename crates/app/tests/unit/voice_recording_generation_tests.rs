use super::{VoiceControlState, VoiceTrigger};

/// VC-2B：用户在异步 Provider 启动完成前松键后，迟到 session 不得被安装。
///
/// 数据构造（含关键数值的推导过程）：
///   recording attempts = 2
///   generation 1 = 启动后立即 finish
///   generation 2 = 后续新录音
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次 begin → 记录 generation 1
///   2. Provider 尚未返回时 finish → phase 进入 Transcribing 并关闭启动接收窗
///   3. 第二次 begin → 产生严格更新的 generation 2
///
/// 预期结果：
///   - 正断言：generation 2 可接收自己的启动结果
///   - 负断言：松键后的 generation 1 不得再安装 session 或接收最终文本
#[test]
fn test_stale_recording_start_and_result_are_rejected_after_release() {
    let mut state = VoiceControlState::default();

    assert!(
        state.begin_recording(VoiceTrigger::Shortcut),
        "第一次录音尝试应成功开始"
    );
    let first_generation = state.recording_generation();
    assert!(
        state.accepts_recording_start(first_generation),
        "仍处于 Recording 时应接收同 generation 的启动结果"
    );
    assert!(
        state.finish_recording(VoiceTrigger::Shortcut),
        "松键应结束第一次录音尝试"
    );

    assert!(
        !state.accepts_recording_start(first_generation),
        "松键后迟到的 Provider/session 启动结果不得被安装"
    );
    assert!(
        state.accepts_recording_result(first_generation),
        "已真正安装并完成的同 generation 结果仍可在 Transcribing 阶段接收"
    );
    state.set_error("first attempt had no installed session");
    assert!(
        state.begin_recording(VoiceTrigger::Shortcut),
        "错误恢复后应允许第二次录音尝试"
    );
    let second_generation = state.recording_generation();

    assert!(
        second_generation > first_generation,
        "新录音 generation 必须严格递增以隔离迟到结果"
    );
    assert!(
        state.accepts_recording_start(second_generation),
        "第二次录音应只接收自己的启动结果"
    );
    assert!(
        !state.accepts_recording_result(first_generation),
        "第一次录音的迟到 final 不得覆盖第二次录音状态"
    );
}
