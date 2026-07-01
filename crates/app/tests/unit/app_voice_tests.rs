use super::{
    voice_transcript_source_from_value, VoicePlanningJob, VoicePlanningResult,
    VoiceTranscriptSource, DEFAULT_MAX_VOICE_STEPS,
};

/// AV-0: 默认 voice planner 步数应覆盖“四张卡片 + 一次输入”的复合语音。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex card      = 1 create command
///   Claude Code card = 1 create command
///   Terminal cards  = 2 create commands
///   input hi        = 1 send_text command
///   Total           = 1 + 1 + 2 + 1 = 5 commands
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app 传给 VoicePlanningRequest 的 DEFAULT_MAX_VOICE_STEPS
///   2. 与复合语音所需 5 步比较
///
/// 预期结果：
///   - 正断言：默认 max steps 至少为 5，能表达用户的四卡片加输入场景
///   - 负断言：默认 max steps 不应继续是旧的 2，否则 LLM 被 prompt 限死
#[test]
fn test_default_voice_steps_cover_four_cards_then_input() {
    let required_steps = 1 + 1 + 2 + 1;

    assert!(
        DEFAULT_MAX_VOICE_STEPS >= required_steps,
        "默认 voice planner 步数至少应覆盖 4 个 create + 1 个 send_text"
    );
    assert_ne!(
        DEFAULT_MAX_VOICE_STEPS, 2,
        "默认 voice planner 步数不应继续是旧的两步限制"
    );
}

/// AV-1: 未设置开发 fallback 时，语音入口应选择真实 Apple Speech。
///
/// 数据构造（含关键数值的推导过程）：
///   env value = None（等价于未设置 MULTISOUL_VOICE_TRANSCRIPT）
///   expected source = AppleSpeech（需要打开麦克风 provider）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 None 传入 voice_transcript_source_from_value
///   2. helper 根据缺失的 env value 选择 AppleSpeech
///
/// 预期结果：
///   - 正断言：返回 AppleSpeech，普通程序测试路径不再要求 env transcript
///   - 负断言：不应返回 Environment，避免继续走旧模拟入口
#[test]
fn test_voice_source_defaults_to_apple_speech_without_env_transcript() {
    let source = voice_transcript_source_from_value(None);

    assert_eq!(
        source,
        VoiceTranscriptSource::AppleSpeech,
        "未设置 MULTISOUL_VOICE_TRANSCRIPT 时应进入真实 Apple Speech 路径"
    );
    assert_ne!(
        source,
        VoiceTranscriptSource::Environment("hello".to_string()),
        "未设置 env 时不应伪造 Environment transcript"
    );
}

/// AV-2: 设置开发 fallback 时，语音入口应保留模拟 transcript 且不打开麦克风。
///
/// 数据构造（含关键数值的推导过程）：
///   env value = Some("创建一个 Codex 卡片")
///   transcript length > 0（非空 fallback）
///   expected source = Environment("创建一个 Codex 卡片")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 Some(transcript) 传入 voice_transcript_source_from_value
///   2. helper 返回 Environment，并原样携带 transcript
///
/// 预期结果：
///   - 正断言：返回 Environment 且 transcript 原样保留
///   - 负断言：不应进入 AppleSpeech，避免自动化测试打开真实麦克风
#[test]
fn test_voice_source_uses_env_transcript_as_hidden_fallback() {
    let transcript = "创建一个 Codex 卡片".to_string();
    let source = voice_transcript_source_from_value(Some(transcript.clone()));

    assert_eq!(
        source,
        VoiceTranscriptSource::Environment(transcript),
        "设置 MULTISOUL_VOICE_TRANSCRIPT 时应走隐藏模拟 transcript 入口"
    );
    assert_ne!(
        source,
        VoiceTranscriptSource::AppleSpeech,
        "设置 env fallback 时不应打开 Apple Speech 麦克风 provider"
    );
}

fn assert_send<T: Send>() {}

/// AV-3: 语音 planning job 的输入与输出必须可跨线程移动。
///
/// 数据构造（含关键数值的推导过程）：
///   finalizer 在 background executor 完成后，会把 raw transcript 交给 planning job
///   planning job 内含 VoicePlanningRequest + VoiceSettingsValues + transcript 字符串
///   background executor 要求 job capture 和 result 都满足 Send
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译期调用 assert_send::<VoicePlanningJob>()
///   2. 编译期调用 assert_send::<VoicePlanningResult>()
///   3. 若 job/result 捕获 Window、Context、MultisoulApp 或非 Send 状态，测试无法编译
///
/// 预期结果：
///   - 正断言：VoicePlanningJob 可 Send，可进入 background executor
///   - 正断言：VoicePlanningResult 可 Send，可从后台返回 UI task
///   - 负断言：planning job 不应携带 GPUI Window/Context 或整个 app 状态
#[test]
fn test_voice_planning_job_and_result_are_send() {
    assert_send::<VoicePlanningJob>();
    assert_send::<VoicePlanningResult>();
}
