use super::{
    evaluate_voice_readiness, VoiceEntryState, VoiceReadinessInputs, VoiceRequirement,
    VoiceRequirementState,
};

/// 首次启动缺少全部必需能力：Voice 入口必须引导设置，并优先修复麦克风权限
///
/// 数据构造（含关键数值的推导过程）：
///   4 个必需项 = 麦克风 + ASR + Provider + TTS
///   Ready 项数 = 0
///   Missing 项数 = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 传入四项 Missing，且 setup_was_completed = false
///   2. readiness 按固定顺序寻找第一个未完成项
///   3. 返回首次设置状态，不允许进入录音就绪态
///
/// 预期结果：
///   - 正断言：状态为 SetUp，首个未完成项是 Microphone
///   - 负断言：状态不是 Ready，也不是已完成后失效的 Unavailable
#[test]
fn fresh_install_requires_setup_from_microphone() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Missing,
        speech_recognition: VoiceRequirementState::Missing,
        agent_provider: VoiceRequirementState::Missing,
        spoken_replies: VoiceRequirementState::Missing,
        model_install_progress: None,
        setup_was_completed: false,
    });

    assert_eq!(
        state,
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::Microphone,
        },
        "fresh Voice setup must open at the microphone requirement"
    );
    assert_ne!(
        state,
        VoiceEntryState::Ready,
        "fresh Voice setup must not be considered ready"
    );
}

/// TTS 缺失而其余三项可用：TTS 仍是硬门槛，不能以纯文字模式完成设置
///
/// 数据构造（含关键数值的推导过程）：
///   4 个必需项中 Ready = 3
///   SpokenReplies = Missing
///   完成率 = 3 / 4 = 75%，但没有安装任务正在运行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 麦克风、ASR、Provider 标记 Ready
///   2. TTS 标记 Missing，setup_was_completed = false
///   3. readiness 返回 SetUp 并定位 SpokenReplies
///
/// 预期结果：
///   - 正断言：首个未完成项为 SpokenReplies
///   - 负断言：不得返回 Ready
#[test]
fn tts_is_a_required_readiness_gate() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Ready,
        speech_recognition: VoiceRequirementState::Ready,
        agent_provider: VoiceRequirementState::Ready,
        spoken_replies: VoiceRequirementState::Missing,
        model_install_progress: None,
        setup_was_completed: false,
    });

    assert_eq!(
        state,
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::SpokenReplies,
        },
        "missing TTS must keep Voice in setup and focus Spoken Replies"
    );
    assert_ne!(
        state,
        VoiceEntryState::Ready,
        "text-only replies must not satisfy the required TTS gate"
    );
}

/// 推荐模型包在后台下载：入口只按正在下载的模型字节聚合进度
///
/// 数据构造（含关键数值的推导过程）：
///   ASR Installing = 256 / 512 = 50%
///   TTS Installing = 64 / 128 = 50%
///   聚合进度 = (256 + 64) / (512 + 128) = 320 / 640 = 50%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 两个模型项提供各自 downloaded / total
///   2. readiness 只聚合 ASR/TTS 当前下载字节，不计麦克风和 Provider
///   3. 任一模型项 Installing 时返回 SettingUp
///
/// 预期结果：
///   - 正断言：状态为 SettingUp 50%
///   - 负断言：下载未完成时不得返回 Ready
#[test]
fn active_installation_reports_aggregate_progress() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Ready,
        speech_recognition: VoiceRequirementState::Installing {
            completed: 256,
            total: 512,
        },
        agent_provider: VoiceRequirementState::Ready,
        spoken_replies: VoiceRequirementState::Installing {
            completed: 64,
            total: 128,
        },
        model_install_progress: Some((320, 640)),
        setup_was_completed: false,
    });

    assert_eq!(
        state,
        VoiceEntryState::SettingUp { percent: 50 },
        "two half-complete model downloads must report 50 percent by aggregate bytes"
    );
    assert_ne!(
        state,
        VoiceEntryState::Ready,
        "active model installation must not expose Voice as ready"
    );
}

/// Provider 正在连接但没有模型下载：入口保持配置态，不显示虚假的下载百分比
///
/// 数据构造（含关键数值的推导过程）：
///   ASR Ready = 1，TTS Ready = 1
///   Provider Connecting = 0 / 1（连接步骤，不是下载字节）
///   活跃模型下载数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将麦克风、ASR、TTS 标记 Ready
///   2. 将 Provider 标记 Installing，模拟后台连接验证
///   3. readiness 搜索下载任务与首个未完成项
///
/// 预期结果：
///   - 正断言：状态仍为 SetUp，并定位 AgentProvider
///   - 负断言：不得把网络连接伪装成 SettingUp 0% 模型下载
#[test]
fn provider_connection_does_not_report_model_download_progress() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Ready,
        speech_recognition: VoiceRequirementState::Ready,
        agent_provider: VoiceRequirementState::Installing {
            completed: 0,
            total: 1,
        },
        spoken_replies: VoiceRequirementState::Ready,
        model_install_progress: None,
        setup_was_completed: false,
    });

    assert_eq!(
        state,
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::AgentProvider,
        },
        "Provider connection must remain a setup step rather than model download progress"
    );
    assert_ne!(
        state,
        VoiceEntryState::SettingUp { percent: 0 },
        "non-download Provider work must not show a fake Setting up Voice percentage"
    );
}

/// 曾经完成设置后 Provider 失效：入口必须进入修复态，而不是伪装成首次设置
///
/// 数据构造（含关键数值的推导过程）：
///   setup_was_completed = true
///   Ready 项数 = 3
///   Failed 项数 = 1（AgentProvider）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保留 setup_was_completed 历史事实
///   2. Provider 运行时验证返回 Failed
///   3. readiness 返回 Unavailable 并直接定位 Provider
///
/// 预期结果：
///   - 正断言：状态为 Unavailable，首个失效项是 AgentProvider
///   - 负断言：不得降级成 SetUp 或 Ready
#[test]
fn completed_setup_with_failed_provider_becomes_unavailable() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Ready,
        speech_recognition: VoiceRequirementState::Ready,
        agent_provider: VoiceRequirementState::Failed,
        spoken_replies: VoiceRequirementState::Ready,
        model_install_progress: None,
        setup_was_completed: true,
    });

    assert_eq!(
        state,
        VoiceEntryState::Unavailable {
            first_incomplete: VoiceRequirement::AgentProvider,
        },
        "a previously completed setup with a failed provider must enter repair state"
    );
    assert_ne!(
        state,
        VoiceEntryState::Ready,
        "a failed provider must prevent Voice from remaining ready"
    );
}

/// 四项必需能力全部可用：Voice 才能进入可录音状态
///
/// 数据构造（含关键数值的推导过程）：
///   Ready 项数 = 4
///   Missing / Installing / Failed 项数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 四项均传入 Ready
///   2. readiness 未找到任何未完成项或安装任务
///   3. 返回唯一的 Ready 状态
///
/// 预期结果：
///   - 正断言：状态恰好为 Ready
///   - 负断言：状态不是 SetUp
#[test]
fn all_required_capabilities_make_voice_ready() {
    let state = evaluate_voice_readiness(VoiceReadinessInputs {
        microphone: VoiceRequirementState::Ready,
        speech_recognition: VoiceRequirementState::Ready,
        agent_provider: VoiceRequirementState::Ready,
        spoken_replies: VoiceRequirementState::Ready,
        model_install_progress: None,
        setup_was_completed: false,
    });

    assert_eq!(
        state,
        VoiceEntryState::Ready,
        "Voice must become ready only when all four required capabilities are ready"
    );
    assert_ne!(
        state,
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::Microphone,
        },
        "a fully ready Voice configuration must not remain in setup"
    );
}
