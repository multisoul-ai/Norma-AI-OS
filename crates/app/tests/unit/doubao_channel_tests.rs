use super::*;
use std::time::{Duration, Instant};

fn wanted(now: Instant) -> ChannelInputs<'static> {
    ChannelInputs {
        doubao_mode: true,
        credential: true,
        project: Some("/tmp/norma-project"),
        window: Some(1),
        now,
    }
}

fn bound(state: &mut DoubaoVoiceState, project: &str, window: Option<u64>) {
    state.cancellation = Some(tokio_util::sync::CancellationToken::new());
    state.binding = Some(ChannelBinding {
        project: Some(project.into()),
        window,
    });
}

/// 豆包项目打开后应自动启动常驻通道，不需要 Cmd+E。
///
/// 数据构造：豆包模式 + 已保存凭据 + 已打开项目 + 存在项目窗口；通道未启动，无退避。
/// 执行过程：用这些条件计算通道动作。
/// 预期结果：返回 Start；缺少任一前提（模式、凭据、项目、窗口）时返回 Keep，不能启动。
#[test]
fn doubao_channel_starts_when_project_is_ready() {
    let now = Instant::now();
    let state = DoubaoVoiceState::default();
    assert_eq!(
        channel_action(&state, &wanted(now)),
        ChannelAction::Start,
        "豆包项目就绪时应自动启动播报通道"
    );
    for (label, inputs) in [
        (
            "非豆包模式",
            ChannelInputs {
                doubao_mode: false,
                ..wanted(now)
            },
        ),
        (
            "缺少凭据",
            ChannelInputs {
                credential: false,
                ..wanted(now)
            },
        ),
        (
            "未打开项目",
            ChannelInputs {
                project: None,
                ..wanted(now)
            },
        ),
        (
            "没有窗口",
            ChannelInputs {
                window: None,
                ..wanted(now)
            },
        ),
    ] {
        assert_eq!(
            channel_action(&state, &inputs),
            ChannelAction::Keep,
            "{label}时不能启动通道"
        );
    }
}

/// 启动中、退避期内或被永久阻断时不能重复启动。
///
/// 数据构造：退避截止 = now + 2 s；另构造 start_pending 与 blocked 两种状态。
/// 执行过程：分别在截止前、截止后计算动作，再计算 pending、blocked 状态。
/// 预期结果：截止前 Keep，截止后 Start；pending 与 blocked 均为 Keep。
#[test]
fn doubao_channel_respects_pending_backoff_and_block() {
    let now = Instant::now();
    let mut state = DoubaoVoiceState {
        retry_at: Some(now + Duration::from_secs(2)),
        ..Default::default()
    };
    assert_eq!(
        channel_action(&state, &wanted(now)),
        ChannelAction::Keep,
        "退避截止前不能重连"
    );
    assert_eq!(
        channel_action(&state, &wanted(now + Duration::from_secs(2))),
        ChannelAction::Start,
        "退避截止后应重连"
    );
    state.retry_at = None;
    state.start_pending = true;
    assert_eq!(
        channel_action(&state, &wanted(now)),
        ChannelAction::Keep,
        "已在启动中时不能再排一次启动"
    );
    state.start_pending = false;
    state.blocked = true;
    assert_eq!(
        channel_action(&state, &wanted(now)),
        ChannelAction::Keep,
        "鉴权类失败后不能自动重连"
    );
}

/// 已运行的通道：设置失效则停止；换项目或换窗口则重建；转入后台则保持。
///
/// 数据构造：通道绑定项目 /tmp/norma-project 与窗口 1。
/// 执行过程：依次改为非豆包模式、项目 /tmp/other、窗口 2、窗口 None，以及无绑定的注入通道。
/// 预期结果：Stop、Restart、Restart、Keep、Keep；绑定不变时 Keep。
#[test]
fn doubao_channel_tracks_settings_project_and_window() {
    let now = Instant::now();
    let mut state = DoubaoVoiceState::default();
    bound(&mut state, "/tmp/norma-project", Some(1));
    assert_eq!(
        channel_action(&state, &wanted(now)),
        ChannelAction::Keep,
        "绑定一致时应保持常驻"
    );
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                doubao_mode: false,
                ..wanted(now)
            }
        ),
        ChannelAction::Stop,
        "切换到其他语音模式时应关闭通道"
    );
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                project: Some("/tmp/other"),
                ..wanted(now)
            }
        ),
        ChannelAction::Restart,
        "切换项目时应按新项目重建通道"
    );
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                window: Some(2),
                ..wanted(now)
            }
        ),
        ChannelAction::Restart,
        "窗口重新打开时应重建，让工具调用绑定新窗口"
    );
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                window: None,
                ..wanted(now)
            }
        ),
        ChannelAction::Keep,
        "项目转入后台时应保留通道继续播报"
    );
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                window: None,
                project: Some("/tmp/other"),
                ..wanted(now)
            }
        ),
        ChannelAction::Stop,
        "没有窗口时换项目只能停止旧通道"
    );
    state.binding = None;
    assert_eq!(
        channel_action(
            &state,
            &ChannelInputs {
                project: Some("/tmp/other"),
                ..wanted(now)
            }
        ),
        ChannelAction::Keep,
        "没有绑定信息的通道（测试注入）不能被误判为过期"
    );
}

/// 断线退避按 1、2、4…秒翻倍，上限 60 秒。
///
/// 数据构造：attempt 0..=7 → 2^attempt 秒；2^6 = 64 > 60 截断为 60。
/// 执行过程：逐个计算 retry_delay。
/// 预期结果：1, 2, 4, 8, 16, 32, 60, 60 秒。
#[test]
fn doubao_channel_retry_delay_doubles_up_to_sixty_seconds() {
    let delays: Vec<u64> = (0..=7)
        .map(|attempt| retry_delay(attempt).as_secs())
        .collect();
    assert_eq!(
        delays,
        vec![1, 2, 4, 8, 16, 32, 60, 60],
        "退避应翻倍并封顶 60 秒"
    );
    assert_eq!(retry_delay(u32::MAX).as_secs(), 60, "极大重试次数不能溢出");
}

/// 鉴权与凭据类失败不自动重连；网络类失败自动重连。
///
/// 数据构造：auth_failed 与 connection_failed 两个断开原因，起始 attempt = 0。
/// 执行过程：分别调用 note_channel_ended。
/// 预期结果：auth_failed → blocked 且无 retry_at；connection_failed → retry_at = now + 1 s，attempt = 1。
#[test]
fn doubao_channel_end_classifies_permanent_failures() {
    let now = Instant::now();
    let mut state = DoubaoVoiceState::default();
    state.note_channel_ended(&LocalizedText::new("doubao.auth_failed"), now);
    assert!(state.blocked, "鉴权失败应阻断自动重连");
    assert_eq!(state.retry_at, None, "阻断后不应排重连");

    let mut state = DoubaoVoiceState::default();
    state.note_channel_ended(&LocalizedText::new("doubao.connection_failed"), now);
    assert!(!state.blocked, "网络失败不能阻断");
    assert_eq!(
        state.retry_at,
        Some(now + Duration::from_secs(1)),
        "首次网络失败应在 1 秒后重连"
    );
    assert_eq!(state.retry_attempt, 1, "重连次数应递增");
    state.stop();
    assert_eq!(
        (state.retry_attempt, state.retry_at, state.blocked),
        (0, None, false),
        "显式停止（例如设置变更）应清空退避与阻断"
    );
}

/// 待命与收音分离：通道已连接但未收音时为待命；任一收音令牌存在时为收音中。
///
/// 数据构造：status = Connected；分别设置 recording、listening。
/// 执行过程：检查 waiting_for_input 与 is_listening。
/// 预期结果：无令牌 → 待命；有 recording 或 listening → 收音中，不是待命。
#[test]
fn doubao_channel_separates_standby_from_listening() {
    let mut state = DoubaoVoiceState {
        status: DoubaoStatus::Connected,
        ..Default::default()
    };
    assert!(state.waiting_for_input(), "通道就绪未收音时应显示待命");
    assert!(!state.is_listening(), "待命时不能显示收音中");
    state.listening = Some(tokio_util::sync::CancellationToken::new());
    assert!(state.is_listening(), "持续聆听令牌存在时应为收音中");
    assert!(!state.waiting_for_input(), "收音中不能显示待命");
    state.listening = None;
    state.recording = Some(tokio_util::sync::CancellationToken::new());
    assert!(state.is_listening(), "按住录音时应为收音中");
}

/// 收音结束事件只结束对应那次收音，通道保持连接；失败时进入收音失败状态。
///
/// 数据构造：recording_id = 3，listening 令牌存在。
/// 执行过程：先应用 id 2 的结束（过期），再应用 id 3 的失败结束。
/// 预期结果：过期事件不影响；id 3 取消 listening 令牌、状态变为 ListenFailed，通道 cancellation 仍在。
#[test]
fn doubao_channel_listen_end_only_stops_matching_listen() {
    let listening = tokio_util::sync::CancellationToken::new();
    let mut state = DoubaoVoiceState {
        status: DoubaoStatus::Connected,
        recording_id: 3,
        listening: Some(listening.clone()),
        cancellation: Some(tokio_util::sync::CancellationToken::new()),
        ..Default::default()
    };
    state.apply_status(DoubaoStatus::ListenEnded(2, None));
    assert!(
        state.listening.is_some(),
        "旧收音的结束事件不能停止当前收音"
    );
    let error = LocalizedText::new("doubao.asr_disconnected");
    state.apply_status(DoubaoStatus::ListenEnded(3, Some(error.clone())));
    assert!(state.listening.is_none(), "当前收音结束后应清除令牌");
    assert!(listening.is_cancelled(), "结束收音应取消令牌，释放麦克风");
    assert_eq!(
        state.status,
        DoubaoStatus::ListenFailed(error),
        "收音失败应显示失败状态"
    );
    assert!(state.is_active(), "收音失败不能关闭播报通道");
}

/// 稳定连接后断开，退避从 1 秒重新开始；连上即断仍按次数翻倍。
///
/// 数据构造：retry_attempt = 6（下次本应等 2^6 → 60 s）；稳定阈值 30 s。
///   A：connected_at = now − 31 s（已稳定）；B：connected_at = now − 1 s（未稳定）。
/// 执行过程：先验证 Connected 状态会记录连接时刻，再分别对 A、B 调用 note_channel_ended。
/// 预期结果：A → retry_at = now + 1 s、attempt = 1；B → retry_at = now + 60 s、attempt = 7。
#[test]
fn doubao_channel_backoff_resets_after_stable_connection() {
    let mut fresh = DoubaoVoiceState::default();
    fresh.apply_status(DoubaoStatus::Connected);
    assert!(fresh.connected_at.is_some(), "通道就绪应记录连接时刻");

    let now = Instant::now();
    let reason = LocalizedText::new("doubao.connection_failed");
    let mut stable = DoubaoVoiceState {
        retry_attempt: 6,
        connected_at: Some(now - Duration::from_secs(31)),
        ..Default::default()
    };
    stable.note_channel_ended(&reason, now);
    assert_eq!(
        (stable.retry_at, stable.retry_attempt),
        (Some(now + Duration::from_secs(1)), 1),
        "稳定连接后的断线应从 1 秒重新退避"
    );

    let mut flapping = DoubaoVoiceState {
        retry_attempt: 6,
        connected_at: Some(now - Duration::from_secs(1)),
        ..Default::default()
    };
    flapping.note_channel_ended(&reason, now);
    assert_eq!(
        (flapping.retry_at, flapping.retry_attempt),
        (Some(now + Duration::from_secs(60)), 7),
        "连上即断仍应继续翻倍退避"
    );
}
