use super::{
    RealtimeConfigError, RealtimeControlState, VoiceBarMouseDownAction, REALTIME_PROCESS_TEST_LOCK,
};
use crate::app::voice::control::VoiceVisual;
use realtime::{RealtimeSessionConfig, RealtimeSessionId};

fn config(session_id: &str, model: &str) -> RealtimeSessionConfig {
    RealtimeSessionConfig {
        session_id: RealtimeSessionId::new(session_id),
        model: model.to_string(),
        tools: Vec::new(),
    }
}

fn with_realtime_process_guard(assertions: impl FnOnce()) {
    let _guard = REALTIME_PROCESS_TEST_LOCK
        .lock()
        .expect("realtime process test lock should not be poisoned");
    RealtimeControlState::reset_process_guard_for_tests();
    assertions();
    RealtimeControlState::reset_process_guard_for_tests();
}

/// RT-1: 默认 realtime shell 不覆盖 one-shot voice 的底栏 label 与视觉。
///
/// 数据构造（含关键数值的推导过程）：
///   state = default → status Off；fallback label = "Voice"；fallback visual = Idle
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 state
///   2. 读取 realtime_voice_label / effective label / effective visual
///
/// 预期结果：
///   - 正断言：默认 effective label 是 Voice，默认 visual 是 Idle
///   - 负断言：Off 状态不应显示 Connecting... 或 Always Listening
#[test]
fn test_realtime_default_maps_bottom_bar_to_voice() {
    let state = RealtimeControlState::default();

    assert!(
        state.realtime_voice_label().is_none(),
        "Off 状态不应覆盖 one-shot voice label"
    );
    assert_eq!(
        state.effective_voice_label("Voice"),
        "Voice",
        "默认 realtime shell 应让底栏继续显示 one-shot Voice"
    );
    assert_eq!(
        state.effective_voice_visual(VoiceVisual::Idle),
        VoiceVisual::Idle,
        "默认 realtime shell 应保留 one-shot Idle 视觉"
    );
    assert_ne!(
        state.effective_voice_label("Voice"),
        "Connecting...",
        "Off 状态不应误显示 Connecting..."
    );
    assert_ne!(
        state.effective_voice_label("Voice"),
        "Always Listening",
        "Off 状态不应误显示 Always Listening"
    );
}

/// RT-2: 有效配置启动 shell 后，底栏先显示 Connecting...，连接成功后显示 Always Listening。
///
/// 数据构造（含关键数值的推导过程）：
///   config.session_id = rt-1；model = gpt-realtime-2；tools = []
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_config(rt-1) → session status Connecting
///   2. 读取 effective_voice_label("Voice") → Connecting...
///   3. mark_started() → session status Listening
///   4. 再次读取 effective_voice_label("Voice") → Always Listening
///
/// 预期结果：
///   - 正断言：Connecting 与 Listening label 分别正确
///   - 正断言：Listening 使用 Recording 类视觉，让 bottom bar 表示 realtime 正在占用语音入口
///   - 负断言：Listening 后不应退回 one-shot Voice
#[test]
fn test_realtime_valid_config_maps_bottom_label_connecting_then_always_listening() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();

        state
            .start_shell_with_config(config("rt-1", "gpt-realtime-2"))
            .expect("valid shell config should enter Connecting");

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Connecting...",
            "shell start 后底栏应先显示 Connecting..."
        );

        state.mark_started();

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "mark_started 后底栏应显示 Always Listening"
        );
        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Recording,
            "Listening 状态应使用 active voice visual，而不是 Idle"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Voice",
            "Listening 状态不应退回 one-shot Voice label"
        );
    });
}

/// RT-3: 配置错误应映射为 Realtime error，而不是泄露底层设置值。
///
/// 数据构造（含关键数值的推导过程）：
///   error = MissingApiKey
///   fallback label = "Voice"
///   fallback visual = VoiceVisual::Idle
///
/// 执行过程（逐步说明系统如何处理）：
///   1. RealtimeControlState::default()
///   2. mark_config_error(MissingApiKey)
///   3. 读取 effective label / visual / Debug
///
/// 预期结果：
///   - 正断言：label 映射为 Realtime error，visual 映射为 Error
///   - 负断言：错误状态不应继续显示 Voice 或 Connecting...
#[test]
fn test_realtime_config_error_maps_bottom_label_to_realtime_error() {
    let mut state = RealtimeControlState::default();

    state.mark_config_error(RealtimeConfigError::MissingApiKey);

    assert_eq!(
        state.effective_voice_label("Voice"),
        "Realtime error",
        "配置错误时底栏应显示 Realtime error"
    );
    assert_eq!(
        state.effective_voice_visual(VoiceVisual::Idle),
        VoiceVisual::Error,
        "配置错误时底栏应使用 Error 视觉"
    );
    assert_ne!(
        state.effective_voice_label("Voice"),
        "Voice",
        "配置错误不应静默退回 one-shot Voice"
    );
    assert_ne!(
        state.effective_voice_label("Voice"),
        "Connecting...",
        "配置错误不应显示 Connecting..."
    );
}

/// RT-3b: 缺少 sidecar path 的配置错误必须进入 Realtime error，且不能抢占进程级 realtime slot。
///
/// 数据构造（含关键数值的推导过程）：
///   config_error = MissingSidecarPath
///   fallback label = "Voice"
///   retry session_id = rt-after-missing-sidecar
///   process slot capacity = 1，但当前错误路径不应获取任何 slot
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认 state 调用 mark_config_error(MissingSidecarPath)
///   2. 读取 label 与 current_session_id
///   3. 新建第二个独立 state，并尝试 start_shell_with_config(retry)
///
/// 预期结果：
///   - 正断言：错误 label 显示 Realtime error，current_session_id 为空
///   - 正断言：第二个 state 可以成功启动，证明缺 sidecar path 没有占用进程级 slot
///   - 负断言：错误状态不应残留 active session，也不应继续显示 Voice
#[test]
fn test_realtime_missing_sidecar_path_error_does_not_acquire_process_slot() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        state.mark_config_error(RealtimeConfigError::MissingSidecarPath);

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Realtime error",
            "缺少 sidecar path 时底栏应显示 Realtime error"
        );
        assert!(
            state.current_session_id().is_none(),
            "缺少 sidecar path 的配置错误不应保留 active realtime session id"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Voice",
            "缺少 sidecar path 的配置错误不应静默退回 Voice"
        );

        let mut retry = RealtimeControlState::default();
        let retry_start =
            retry.start_shell_with_config(config("rt-after-missing-sidecar", "gpt-realtime-2"));
        assert!(
            retry_start.is_ok(),
            "缺少 sidecar path 的错误路径不应占用进程级 slot，后续独立 state 应可启动"
        );
        retry.stop_shell();
    });
}

/// RT-4: Realtime active 时，bottom-bar mouse down 不应启动 one-shot recording。
///
/// 数据构造（含关键数值的推导过程）：
///   realtime_enabled=false + inactive → StartOneShotRecording（保留关闭 realtime 时的旧行为）
///   realtime_enabled=true + inactive → StartAlwaysListeningShell（点击启动 shell）
///   realtime_enabled=true + active Connecting → Ignore（active 时不重复启动，也不走 one-shot）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认 state 读取 mouse_down_action(false)
///   2. 默认 state 读取 mouse_down_action(true)
///   3. start_shell_with_config 后读取 mouse_down_action(true)
///
/// 预期结果：
///   - 正断言：realtime 关闭时仍走 one-shot
///   - 正断言：realtime 开启但未 active 时启动 shell
///   - 负断言：active realtime 时不返回 StartOneShotRecording
#[test]
fn test_realtime_active_bottom_bar_mouse_down_does_not_start_one_shot_recording() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();

        assert_eq!(
            state.voice_bar_mouse_down_action(false),
            VoiceBarMouseDownAction::StartOneShotRecording,
            "realtime 关闭时 bottom bar mouse down 应保留 one-shot 行为"
        );
        assert_eq!(
            state.voice_bar_mouse_down_action(true),
            VoiceBarMouseDownAction::StartAlwaysListeningShell,
            "realtime 开启且未 active 时 bottom bar mouse down 应启动 shell"
        );

        state
            .start_shell_with_config(config("rt-active", "gpt-realtime-2"))
            .expect("valid shell config should enter Connecting");

        assert_eq!(
            state.voice_bar_mouse_down_action(true),
            VoiceBarMouseDownAction::Ignore,
            "realtime active 时 bottom bar mouse down 应忽略重复开始"
        );
        assert_ne!(
            state.voice_bar_mouse_down_action(true),
            VoiceBarMouseDownAction::StartOneShotRecording,
            "realtime active 时 bottom bar mouse down 不应启动 one-shot recording"
        );
    });
}

/// RT-5: active session 上重复 start 不应创建第二个 session。
///
/// 数据构造（含关键数值的推导过程）：
///   first config.session_id = rt-first
///   duplicate config.session_id = rt-second
///   active condition = first start 后 status Connecting，session_id=rt-first
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_config(rt-first) → Ok，保存 rt-first
///   2. start_shell_with_config(rt-second) → Err AlreadyActive
///   3. 读取 current_session_id
///
/// 预期结果：
///   - 正断言：第一次启动成功，第二次启动失败
///   - 正断言：current_session_id 仍是 rt-first
///   - 负断言：current_session_id 不应被替换成 rt-second
#[test]
fn test_realtime_repeating_start_while_active_does_not_create_second_session() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();

        let first = state.start_shell_with_config(config("rt-first", "gpt-realtime-2"));
        let duplicate = state.start_shell_with_config(config("rt-second", "gpt-realtime-2"));

        assert!(first.is_ok(), "第一次 shell start 应成功进入 Connecting");
        assert!(
            duplicate.is_err(),
            "active session 上重复 start 应返回错误，而不是创建第二个 session"
        );
        assert_eq!(
            state.current_session_id().map(|id| id.as_str()),
            Some("rt-first"),
            "重复 start 后 current_session_id 应保留第一个 session"
        );
        assert_ne!(
            state.current_session_id().map(|id| id.as_str()),
            Some("rt-second"),
            "重复 start 不应把 active session 替换为第二个 session"
        );
    });
}

/// RT-6: 进程级 guard 应阻止两个独立 app/window state 同时进入 active realtime session。
///
/// 数据构造（含关键数值的推导过程）：
///   first.state = RealtimeControlState::default()
///   second.state = RealtimeControlState::default()
///   first session_id = rt-first-window
///   second session_id = rt-second-window
///   active session capacity = 1（全进程只能有一个 active realtime session）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. first.start_shell_with_config(rt-first-window) → Ok，持有进程级 slot
///   2. second.start_shell_with_config(rt-second-window) → Err AlreadyActive
///   3. first.stop_shell() 释放进程级 slot
///   4. second.start_shell_with_config(rt-second-window) → Ok
///
/// 预期结果：
///   - 正断言：第一个独立 state 可启动，第二个独立 state 在 first active 时失败
///   - 正断言：失败错误指向 first session id，second 不获得本地 session id
///   - 正断言：first stop 后 second 可启动
///   - 负断言：second 不应在 first active 时替换成 rt-second-window
#[test]
fn test_realtime_process_guard_blocks_second_independent_state_until_first_stops() {
    with_realtime_process_guard(|| {
        let mut first = RealtimeControlState::default();
        let mut second = RealtimeControlState::default();

        first
            .start_shell_with_config(config("rt-first-window", "gpt-realtime-2"))
            .expect("first realtime state should acquire the process slot");
        let second_start =
            second.start_shell_with_config(config("rt-second-window", "gpt-realtime-2"));

        let error = second_start
            .expect_err("second independent realtime state must not acquire a second process slot");
        assert!(
            format!("{error:?}").contains("rt-first-window"),
            "second start error should point at the process-active first session: {error:?}"
        );
        assert!(
            second.current_session_id().is_none(),
            "second state must remain inactive while first owns the process slot"
        );
        assert_ne!(
            second.current_session_id().map(|id| id.as_str()),
            Some("rt-second-window"),
            "second state must not replace the process slot while first is active"
        );

        first.stop_shell();
        let second_retry =
            second.start_shell_with_config(config("rt-second-window", "gpt-realtime-2"));

        assert!(
            second_retry.is_ok(),
            "after first stop_shell releases the process slot, second state should start"
        );
        assert_eq!(
            second.current_session_id().map(|id| id.as_str()),
            Some("rt-second-window"),
            "second state should own rt-second-window after retry"
        );
        second.stop_shell();
    });
}

/// RT-6: Always Listening 开关与生命周期必须接入 start/stop path。
///
/// 数据构造（含关键数值的推导过程）：
///   settings toggle source = settings/voice/view/realtime.rs
///   workspace open source = workspace/flow.rs
///   app drop source = app/lifecycle.rs
///   required start anchor = start_always_listening_shell(cx)
///   required stop anchor = realtime.stop_shell()
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 静态读取 Settings realtime toggle handler 源码
///   2. 静态读取 workspace open 与 Drop 源码
///   3. 检查开启开关会调用 start path，项目切换和 app drop 会调用 stop path
///
/// 预期结果：
///   - 正断言：Settings 开关开启路径调用 start_always_listening_shell
///   - 正断言：workspace open / app drop 均调用 realtime.stop_shell()
///   - 负断言：开关开启路径不应只有 cx.notify()
#[test]
fn test_realtime_toggle_and_lifecycle_are_wired_to_start_and_stop_paths() {
    let toggle_source = include_str!("../../src/settings/voice/view/realtime.rs");
    let workspace_source = include_str!("../../src/workspace/flow.rs");
    let lifecycle_source = include_str!("../../src/app/lifecycle.rs");

    assert!(
        toggle_source.contains("self.start_always_listening_shell(cx)"),
        "Enable Always Listening 开关开启后必须调用 start_always_listening_shell"
    );
    assert!(
        workspace_source.contains("self.realtime.stop_shell()"),
        "打开/切换 project 时必须停止 Always Listening shell 状态"
    );
    assert!(
        lifecycle_source.contains("self.realtime.stop_shell()"),
        "app drop/close 时必须停止 Always Listening shell 状态"
    );
    assert!(
        !toggle_source
            .contains("self.voice_settings.toggle_realtime_enabled();\n        cx.notify();"),
        "开关开启路径不应只保存设置并 notify，否则不会进入 Connecting 或错误状态"
    );
}
