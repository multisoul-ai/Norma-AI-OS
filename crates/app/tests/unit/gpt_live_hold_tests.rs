use super::*;
use crate::settings::voice::{
    EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode, VoiceSettingsState,
};

/// 场景：GPT交互方式有自己的持久化键，不改变豆包或旧语音指令偏好。
/// 构造：隔离库中豆包和旧语音均为按住模式，GPT没有历史选择。
/// 执行：读取默认→选择GPT按住→执行实际保存→重新加载。
/// 预期：GPT默认实时；自身可恢复按住模式；另外两项原值保持。
#[test]
fn live_interaction_mode_persists_without_changing_other_flows() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    for key in ["voice.doubao.interaction_mode", "voice.interaction.mode"] {
        store
            .set_string(key, "single_voice_command")
            .expect("旧偏好");
    }
    let mut settings = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        settings.values().gpt_live_interaction_mode(),
        VoiceInteractionMode::ContinuousListening,
        "GPT新用户默认实时对话"
    );
    settings.select_gpt_live_interaction_mode(VoiceInteractionMode::SingleVoiceCommand);
    for write in settings.take_pending_writes() {
        write.execute().expect("后台保存");
    }
    settings.record_persistence_result(Ok(()));
    let restored = VoiceSettingsState::load_from_store(store.clone());
    assert_eq!(
        restored.values().gpt_live_interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "GPT交互方式独立恢复"
    );
    for key in ["voice.doubao.interaction_mode", "voice.interaction.mode"] {
        assert_eq!(
            store.get_string(key).expect("读原偏好").as_deref(),
            Some("single_voice_command"),
            "GPT切换不改写{key}"
        );
    }
    let defaults = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    assert_eq!(
        defaults.values().interaction_mode(),
        VoiceInteractionMode::SingleVoiceCommand,
        "旧语音指令默认不变"
    );
}

/// 场景：按住输入与主动静音独立，松键后保持已建立通话。
/// 构造：已连接GPT状态、按住模式、可观察的命令通道，不使用网络或麦克风。
/// 执行：按下→主动静音确认→松开→再次按下。
/// 预期：仅发输入门控命令；松键保留通话；再次按键不发送取消静音命令。
#[test]
fn live_hold_release_preserves_call_and_manual_mute() {
    let mut state = GptLiveVoiceState::default();
    let (generation, call) = state.begin();
    state.configure_input(true, false, false);
    state.apply(generation, CallEvent::Connected);
    let (commands, received) = async_channel::bounded(8);
    state.commands = Some(commands);
    state.request_input(true, true).expect("按下快捷键");
    assert!(!state.input_complete(), "按住期间不得开始项目工具规划");
    assert!(
        matches!(
            received.try_recv(),
            Ok(CallCommand::SetInputEnabled { enabled: true, .. })
        ),
        "按下只开启输入门控"
    );
    state.apply(generation, CallEvent::Muted(true));
    state.request_input(false, false).expect("松键");
    assert!(!state.input_complete(), "松键后先等待采集尾音排空");
    state.apply(generation, CallEvent::InputReleased(state.input_revision));
    assert!(state.input_complete(), "松键后可按现有证据规划");
    assert!(
        state.is_active() && !call.is_cancelled(),
        "松键不挂断已建立通话"
    );
    assert!(
        matches!(
            received.try_recv(),
            Ok(CallCommand::SetInputEnabled { enabled: false, .. })
        ),
        "松键关闭输入门控"
    );
    state.request_input(true, true).expect("再次按下");
    assert!(state.is_muted(), "主动静音不会被按住输入覆盖");
    assert!(
        matches!(
            received.try_recv(),
            Ok(CallCommand::SetInputEnabled { enabled: true, .. })
        ),
        "再次按下不发送SetMuted(false)"
    );
    assert!(received.try_recv().is_err(), "不能附带覆盖静音的隐含命令");
}

/// 场景：首次按住连接尚未完成就松键，迟到连接不能恢复收音。
/// 构造：Connecting通话和按住输入状态，不打开硬件。
/// 执行：松开输入后注入旧generation的Connected回调。
/// 预期：取消启动token，旧回调被丢弃，通话不再活跃。
#[test]
fn live_hold_release_during_startup_cancels_late_capture() {
    let mut state = GptLiveVoiceState::default();
    let (generation, call) = state.begin();
    state.configure_input(true, true, true);
    state.request_input(false, false).expect("冷启动松键");
    assert!(call.is_cancelled(), "松键立即撤销尚未完成的启动");
    assert!(!state.is_active(), "不能保留即将迟到开启的麦克风");
    assert!(
        !state.apply(generation, CallEvent::Connected),
        "旧连接回调不得复活录音"
    );
}

/// 场景：补充语音的转写尚未到达，松键不能令旧创建授权重新执行。
/// 构造：旧委派已规划；绑定实际Bridge时间线，按住模式已连接，不推进VAD或转写。
/// 执行：按下撤销→松开→旧无副作用规划完成→尝试认领→补充原文到达。
/// 预期：补字前不能重排执行旧原文；补字到达后才按完整请求继续，意图身份保持。
#[test]
fn live_hold_release_waits_for_new_transcript_before_replanning() {
    use super::super::{bridge::LiveBridge, report::DelegatedReport};
    let mut bridge = LiveBridge::new("hold-call".into(), CancellationToken::new());
    let fragment = |id: &str, text: &str, start: f64| live_voice::TranscriptFragment {
        event_id: id.into(),
        speaker: live_voice::Speaker::User,
        delta: text.into(),
        start_ms: start,
        end_ms: start + 100.0,
    };
    bridge
        .transcript(fragment("u1", "创建一个终端", 0.0))
        .expect("旧原文");
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d1".into(),
            offset_ms: 100.0,
        })
        .expect("委派");
    let work = bridge.claim_next().expect("旧规划");
    let mut state = GptLiveVoiceState::default();
    let (generation, _) = state.begin();
    state.configure_input(true, false, false);
    state.apply(generation, CallEvent::Connected);
    state.bind_timeline(bridge.timeline.clone());
    state.planning = Some(work.cancellation.clone());
    let (commands, _controls) = async_channel::bounded(8);
    state.commands = Some(commands);
    state.request_input(true, true).expect("开始补充");
    state.request_input(false, false).expect("释放按键");
    state.apply(generation, CallEvent::InputReleased(state.input_revision));
    assert!(work.cancellation.is_cancelled(), "按下同步撤销原规划");
    assert!(
        bridge.complete(&work, DelegatedReport::default()).is_none(),
        "过期无副作用规划不口播"
    );
    assert!(
        bridge.claim_next().is_none(),
        "松键不是转写完成证据，不得重做旧原文"
    );
    bridge
        .transcript(fragment("u2", "，先别执行", 100.0))
        .expect("补充原文到达");
    let revised = bridge.claim_next().expect("新证据到达后可继续处理");
    assert_eq!(revised.intent, work.intent, "补充不另造副作用身份");
    assert_eq!(
        revised.snapshot.user_input, "创建一个终端，先别执行",
        "只能按包含取消指令的完整证据重规划"
    );
}
