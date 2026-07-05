use super::{RealtimeControlState, REALTIME_PROCESS_TEST_LOCK};
use crate::app::voice::control::VoiceVisual;
use crate::settings::voice::VoiceSettingsValues;
use realtime::{RealtimeSessionConfig, RealtimeSessionId};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

fn config(session_id: &str, model: &str) -> RealtimeSessionConfig {
    RealtimeSessionConfig {
        session_id: RealtimeSessionId::new(session_id),
        model: model.to_string(),
        tools: Vec::new(),
    }
}

fn values_with_sidecar_path(model: &str, sidecar_path: Option<PathBuf>) -> VoiceSettingsValues {
    VoiceSettingsValues {
        ollama_endpoint: "http://127.0.0.1:11434".to_string(),
        planner_model: "qwen2.5".to_string(),
        refine_model: "qwen2.5".to_string(),
        paraformer_model_dir: None,
        input_device_id: None,
        trace_enabled: false,
        realtime_enabled: true,
        openai_api_key: Some("sk-app-sidecar-secret".to_string()),
        realtime_model: Some(model.to_string()),
        card_planner_model: Some("gpt-card-planner".to_string()),
        sidecar_path,
    }
}

fn fake_sidecar_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/realtime/tests/fixtures/fake_sidecar.mjs")
}

fn wait_for_realtime_drain(state: &mut RealtimeControlState, expected_label: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        state.drain_sidecar_events();
        if state.effective_voice_label("Voice") == expected_label {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "app realtime state did not reach expected label {expected_label} before timeout"
        );
        thread::sleep(Duration::from_millis(10));
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

/// RT-S-1: .mjs sidecar path 启动 fake sidecar，并在 session_started 后进入 Listening。
///
/// 数据构造（含关键数值的推导过程）：
///   sidecar_path = fake_sidecar.mjs；api_key = "sk-app-sidecar-secret"；model = gpt-realtime-2
///   wait timeout = 3s，fixture 应输出 session_started
///
/// 执行过程（逐步说明系统如何处理）：
///   1. validate config，并把 .mjs path 转成 node + fixture arg
///   2. start_shell_with_sidecar_config 启动进程并发送 start_session
///   3. drain_sidecar_events 读取 session_started → mark_started
///
/// 预期结果：
///   - 正断言：args 包含 fake fixture，label 从 Connecting... 变为 Always Listening
///   - 负断言：收到 session_started 前不应直接跳过 Connecting 状态
#[test]
fn test_realtime_sidecar_path_starts_fake_sidecar_then_listens_on_session_started() {
    with_realtime_process_guard(|| {
        let values = values_with_sidecar_path("gpt-realtime-2", Some(fake_sidecar_fixture_path()));
        let config = RealtimeControlState::validate_start_config(&values)
            .expect("valid sidecar settings should produce a realtime session config");
        let sidecar = RealtimeControlState::sidecar_launch_config(&values)
            .expect("settings with sidecar_path should produce a sidecar launch config");
        assert!(
            sidecar
                .args
                .iter()
                .any(|arg| arg.ends_with("fake_sidecar.mjs")),
            "app fake-path support should pass the .mjs fixture as a node argument"
        );

        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_sidecar_config(config, sidecar)
            .expect("valid fake sidecar should start and accept start_session");

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Connecting...",
            "sidecar-backed shell should remain Connecting until session_started is received"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "sidecar-backed shell must not mark Listening before draining session_started"
        );

        wait_for_realtime_drain(&mut state, "Always Listening");

        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Recording,
            "after session_started, sidecar-backed shell should use active Recording visual"
        );
        state.stop_shell();
    });
}

/// RT-S-2: sidecar event decode error 映射为 Realtime error 并释放 guard。
///
/// 数据构造（含关键数值的推导过程）：
///   sidecar_path = fake_sidecar.mjs；model = emit-malformed-json；wait timeout = 3s
///   fixture 输出 "{malformed-json" → 协议层返回 MalformedEvent
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 config + sidecar launch，启动 fake sidecar 并发送 start_session
///   2. drain_sidecar_events 收到 Err(MalformedEvent) → mark_error + release guard
///
/// 预期结果：
///   - 正断言：malformed sidecar event 后 label 显示 Realtime error，visual 显示 Error
///   - 正断言：错误后第二个独立 state 可以重新启动，说明 guard 已释放
///   - 负断言：malformed event 不应让 shell 留在 Always Listening
#[test]
fn test_realtime_sidecar_event_error_maps_to_realtime_error_and_releases_guard() {
    with_realtime_process_guard(|| {
        let values =
            values_with_sidecar_path("emit-malformed-json", Some(fake_sidecar_fixture_path()));
        let session_config = RealtimeControlState::validate_start_config(&values)
            .expect("valid settings should produce config before malformed fixture event");
        let sidecar = RealtimeControlState::sidecar_launch_config(&values)
            .expect("sidecar_path should produce launch config for malformed fixture test");

        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_sidecar_config(session_config, sidecar)
            .expect("fake sidecar should launch before emitting malformed event");

        wait_for_realtime_drain(&mut state, "Realtime error");

        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Error,
            "sidecar MalformedEvent should map to the bottom bar Error visual"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "malformed sidecar event must not leave the shell in Listening state"
        );

        let mut second = RealtimeControlState::default();
        let retry = second.start_shell_with_config(config("rt-after-error", "gpt-realtime-2"));
        assert!(
            retry.is_ok(),
            "sidecar event error should release the process guard for later starts"
        );
        second.stop_shell();
    });
}

/// RT-S-3: sidecar stdout 意外关闭应进入 Realtime error 并释放 guard。
///
/// 数据构造（含关键数值的推导过程）：
///   sidecar_path = fake_sidecar.mjs；model = exit-without-event
///   fixture 收到 start_session 后输出 0 行事件并 exit(0)
///   wait timeout = 3s，覆盖 stdout reader 观察 EOF 的时间
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 sidecar-backed shell，状态进入 Connecting
///   2. fake sidecar 无 terminal event 退出，try_recv 返回 StdoutClosed
///   3. drain_sidecar_events 将状态切到 Realtime error 并释放 process guard
///
/// 预期结果：
///   - 正断言：stdout closed 后底栏显示 Realtime error，visual 显示 Error
///   - 正断言：错误后第二个 state 能启动
///   - 负断言：无 terminal event 退出不应卡在 Connecting
#[test]
fn test_realtime_sidecar_stdout_closed_maps_to_realtime_error_and_releases_guard() {
    with_realtime_process_guard(|| {
        let values =
            values_with_sidecar_path("exit-without-event", Some(fake_sidecar_fixture_path()));
        let session_config = RealtimeControlState::validate_start_config(&values)
            .expect("valid settings should produce config before exit fixture event");
        let sidecar = RealtimeControlState::sidecar_launch_config(&values)
            .expect("sidecar_path should produce launch config for exit fixture test");

        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_sidecar_config(session_config, sidecar)
            .expect("fake sidecar should launch before exiting without event");

        wait_for_realtime_drain(&mut state, "Realtime error");

        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Error,
            "sidecar stdout close should map to the bottom bar Error visual"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Connecting...",
            "sidecar stdout close must not leave the shell stuck in Connecting"
        );

        let mut second = RealtimeControlState::default();
        let retry =
            second.start_shell_with_config(config("rt-after-stdout-close", "gpt-realtime-2"));
        assert!(
            retry.is_ok(),
            "stdout closed error should release the process guard for later starts"
        );
        second.stop_shell();
    });
}

/// RT-S-4: sidecar 错误收口不应在前台同步等待 stop timeout。
///
/// 数据构造（含关键数值的推导过程）：
///   sidecar_path = fake_sidecar.mjs；model = emit-malformed-json
///   sidecar stop timeout = 500ms（app/realtime.rs 常量）
///   foreground budget = 100ms，状态更新路径只能做轻量处理，不能等待 500ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动会输出 malformed JSON 的 fake sidecar
///   2. 循环调用 drain_sidecar_events，直到它处理到 malformed event
///   3. 记录处理 malformed event 的单次 drain 耗时
///
/// 预期结果：
///   - 正断言：处理 malformed event 的 drain 耗时小于 100ms
///   - 正断言：状态仍切到 Realtime error
///   - 负断言：前台 drain 不应接近 500ms stop timeout
#[test]
fn test_realtime_sidecar_error_stop_is_detached_from_foreground_drain() {
    with_realtime_process_guard(|| {
        let values =
            values_with_sidecar_path("emit-malformed-json", Some(fake_sidecar_fixture_path()));
        let session_config = RealtimeControlState::validate_start_config(&values)
            .expect("valid settings should produce config before detach timing test");
        let sidecar = RealtimeControlState::sidecar_launch_config(&values)
            .expect("sidecar_path should produce launch config for detach timing test");
        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_sidecar_config(session_config, sidecar)
            .expect("fake sidecar should launch before malformed detach timing test");

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let started = Instant::now();
            let changed = state.drain_sidecar_events();
            let elapsed = started.elapsed();
            if changed {
                assert!(
                    elapsed < Duration::from_millis(100),
                    "draining malformed sidecar event should detach stop/wait from foreground, elapsed {elapsed:?}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "malformed sidecar event should arrive before timing test deadline"
            );
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Realtime error",
            "detached sidecar stop path should still update the shell to Realtime error"
        );
        assert!(
            state.effective_voice_label("Voice") != "Connecting...",
            "detached stop path must not leave the shell in Connecting while background stop runs"
        );
    });
}
