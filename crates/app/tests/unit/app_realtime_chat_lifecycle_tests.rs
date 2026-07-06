use super::app_realtime_tests_chat_support::{
    chunk, config, wait_until, with_realtime_process_guard, RecordingAudioSink, RecordingPcmStream,
    RecordingSidecar,
};
use super::{chat::RealtimeChatRuntime, RealtimeControlState};
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeSessionId};
use std::time::{Duration, Instant};

/// RTC-5: user PCM while assistant is speaking sends interrupt and stops playback.
///
/// 数据构造（含关键数值的推导过程）：
///   assistant state = speaking true
///   user PCM bytes = [2, 4, 6, 8]
///   expected command order after start = start_session, interrupt, input_audio_chunk
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar chat 并进入 Listening
///   2. 设置 assistant speaking true
///   3. send_input_audio_chunk(user chunk)
///   4. 检查 sidecar commands 与 fake sink stop_calls
///
/// 预期结果：
///   - 正断言：interrupt 发送到同一 session，sink stop 调用一次
///   - 正断言：用户 PCM 仍被转发为 input_audio_chunk
///   - 负断言：bar 不应继续显示 Replying...，避免本地播放被打断后仍认为 assistant 在说话
#[test]
fn test_realtime_chat_user_pcm_interrupts_assistant_and_stops_sink() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        let sidecar = RecordingSidecar::default();
        let sink = RecordingAudioSink::default();
        state.set_assistant_audio_sink_for_tests(Box::new(sink.clone()));
        state
            .start_shell_with_sidecar_for_tests(
                config("rt-chat-interrupt"),
                Box::new(sidecar.clone()),
            )
            .expect("valid fake sidecar should start");
        state.drain_sidecar_events();
        state.apply_sidecar_event_for_tests(RealtimeEvent::AssistantAudioState {
            session_id: RealtimeSessionId::new("rt-chat-interrupt"),
            speaking: true,
        });

        let user_chunk = chunk(vec![2, 4, 6, 8]);
        let sent = state
            .send_input_audio_chunk(user_chunk.clone())
            .expect("interrupting assistant with user PCM should still write to fake sidecar");

        assert!(
            sent,
            "user PCM during assistant speaking should still be sent after interrupt"
        );
        assert!(
            sidecar.commands().contains(&RealtimeCommand::Interrupt {
                session_id: RealtimeSessionId::new("rt-chat-interrupt"),
            }),
            "user PCM during assistant speaking must send interrupt to the same session"
        );
        assert!(
            sidecar
                .commands()
                .contains(&RealtimeCommand::InputAudioChunk {
                    session_id: RealtimeSessionId::new("rt-chat-interrupt"),
                    chunk: user_chunk,
                }),
            "user PCM during assistant speaking should still forward input_audio_chunk"
        );
        assert_eq!(
            sink.stop_calls(),
            1,
            "interrupting assistant playback should stop the assistant audio sink exactly once"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Replying...",
            "after local interrupt, app should not keep showing assistant Replying..."
        );
        state.stop_shell();
    });
}

/// RTC-6: stopping Always Listening stops PCM, sends stop_session, clears session, and restores Voice.
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = rt-chat-stop
///   fake PCM stop count starts at 0
///   fake sidecar stop count starts at 0
///   wait timeout = 1s，用于等待后台 sidecar stop helper 调用 fake stop
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_sidecar_and_pcm_for_tests → active session
///   2. drain_sidecar_events → Listening
///   3. stop_shell()
///   4. 等待 fake PCM/background sidecar stop helper 记录 stop
///
/// 预期结果：
///   - 正断言：PCM stop 调用一次，sidecar 收到 stop_session，session 清空，label 回 Voice
///   - 负断言：stop 后不应残留 active session 或 Replying... label
#[test]
fn test_realtime_chat_stop_stops_pcm_sends_stop_and_restores_voice_label() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        let sidecar = RecordingSidecar::default();
        let pcm = RecordingPcmStream::default();
        state
            .start_shell_with_sidecar_and_pcm_for_tests(
                config("rt-chat-stop"),
                Box::new(sidecar.clone()),
                Box::new(pcm.clone()),
            )
            .expect("valid fake chat should start before stop test");
        state.drain_sidecar_events();

        state.stop_shell();
        wait_until(
            Instant::now() + Duration::from_secs(1),
            || pcm.stop_calls() == 1 && sidecar.stop_calls() == 1,
            "stop_shell should trigger background PCM/sidecar stop before timeout",
        );

        assert_eq!(
            pcm.stop_calls(),
            1,
            "stop_shell should stop the active PCM stream exactly once"
        );
        assert!(
            sidecar.commands().contains(&RealtimeCommand::StopSession {
                session_id: RealtimeSessionId::new("rt-chat-stop"),
            }),
            "stop_shell should send stop_session before dropping the sidecar"
        );
        assert!(
            state.current_session_id().is_none(),
            "stop_shell should clear the active realtime session id"
        );
        assert_eq!(
            state.effective_voice_label("Voice"),
            "Voice",
            "stop_shell should return the bottom label to the fallback Voice label"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Replying...",
            "stop_shell should not leave assistant speaking label active"
        );
    });
}

/// RTC-7: 未安装的 realtime runtime 被丢弃时必须回收 PCM 和 sidecar。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = rt-chat-drop-before-install
///   fake PCM stop count starts at 0
///   fake sidecar stop count starts at 0
///   wait timeout = 1s，用于等待后台 sidecar stop helper 调用 fake stop
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造尚未 install 的 RealtimeChatRuntime(fake sidecar + fake PCM)
///   2. 直接 drop(runtime)，模拟后台启动完成后因未安装而被丢弃
///   3. 等待后台 PCM/sidecar stop helper 记录 stop
///
/// 预期结果：
///   - 正断言：PCM stop 调用一次，sidecar stop 调用一次
///   - 负断言：未安装 runtime 被丢弃时不应留下未停止的 PCM 或 sidecar
#[test]
fn test_realtime_chat_drop_uninstalled_runtime_stops_pcm_and_sidecar() {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let pcm = RecordingPcmStream::default();
        let (_pcm_tx, pcm_chunks) = async_channel::unbounded();
        let runtime = RealtimeChatRuntime {
            sidecar: Some(Box::new(sidecar.clone())),
            pcm_stream: Some(Box::new(pcm.clone())),
            pcm_chunks: Some(pcm_chunks),
        };

        drop(runtime);

        wait_until(
            Instant::now() + Duration::from_secs(1),
            || pcm.stop_calls() == 1 && sidecar.stop_calls() == 1,
            "dropping an uninstalled runtime should stop PCM and sidecar before timeout",
        );

        assert_eq!(
            pcm.stop_calls(),
            1,
            "dropping an uninstalled runtime should stop the PCM stream exactly once"
        );
        assert_eq!(
            sidecar.stop_calls(),
            1,
            "dropping an uninstalled runtime should stop the sidecar exactly once"
        );
    });
}

/// RTC-8: stop/restart 之后旧 startup attempt 必须判定为 stale，不能安装到新会话。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = always-listening-shell（当前实现固定值）
///   first startup token 与 second startup token 必须不同
///   stop_shell() 会清空旧的 pending startup
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_config + begin_chat_startup_attempt() 记录 first token
///   2. stop_shell()，模拟用户在 Connecting 时停止
///   3. 再次 start_shell_with_config + begin_chat_startup_attempt() 记录 second token
///   4. 检查旧 token 与新 token 的 accept 判定
///
/// 预期结果：
///   - 正断言：first token 被拒绝，second token 被接受
///   - 负断言：stop/restart 后旧 startup 结果不应再匹配当前安装路径
#[test]
fn test_realtime_chat_stop_and_restart_rejects_stale_startup_attempt() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        let session_id = RealtimeSessionId::new("always-listening-shell");
        state
            .start_shell_with_config(config("always-listening-shell"))
            .expect("first chat startup should enter Connecting");
        let first_startup = state.begin_chat_startup_attempt();

        state.stop_shell();

        state
            .start_shell_with_config(config("always-listening-shell"))
            .expect("second chat startup should enter Connecting after stop");
        let second_startup = state.begin_chat_startup_attempt();

        assert!(
            !state.should_accept_chat_startup_attempt(first_startup, &session_id),
            "startup attempt recorded before stop_shell should be rejected as stale"
        );
        assert!(
            state.should_accept_chat_startup_attempt(second_startup, &session_id),
            "latest startup attempt should still be accepted for installation"
        );
        assert_ne!(
            first_startup, second_startup,
            "stop and restart should issue a fresh startup attempt token"
        );
        state.stop_shell();
    });
}

/// RTC-9: 前台 cleanup 函数必须把 PCM stop 委托到后台 helper，不能内联阻塞 stop。
///
/// 数据构造（含关键数值的推导过程）：
///   source file = crates/app/src/app/realtime/chat.rs（1 个源文件）
///   target fns  = 2 个：RealtimeChatRuntime::dispose / RealtimeControlState::stop_chat_runtime
///   helper name = 1 个：stop_pcm_stream_in_background
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 realtime/chat.rs 源码文本
///   2. 定位 dispose 与 stop_chat_runtime 的函数体片段
///   3. 断言两个前台 cleanup 函数都调用后台 helper
///   4. 断言两个函数体里都不存在直接的 pcm_stream.stop()
///
/// 预期结果：
///   - 正断言：源码包含 stop_pcm_stream_in_background helper 调用
///   - 正断言：dispose 与 stop_chat_runtime 两个函数体都存在
///   - 负断言：两个前台 cleanup 函数体都不应直接调用 pcm_stream.stop()，避免在 GPUI 前台路径阻塞
#[test]
fn test_realtime_chat_cleanup_source_guard_uses_background_pcm_stop_helper() {
    let source_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/realtime/chat.rs");
    let source = std::fs::read_to_string(&source_path)
        .expect("source guard should be able to read realtime chat source");
    let dispose_start = source
        .find("pub(crate) fn dispose(&mut self)")
        .expect("source guard should find RealtimeChatRuntime::dispose");
    let drop_start = source[dispose_start..]
        .find("impl Drop for RealtimeChatRuntime")
        .map(|offset| dispose_start + offset)
        .expect("source guard should find Drop impl after dispose");
    let dispose_body = &source[dispose_start..drop_start];

    let stop_start = source
        .find("pub(super) fn stop_chat_runtime(&mut self)")
        .expect("source guard should find RealtimeControlState::stop_chat_runtime");
    let send_sidecar_start = source[stop_start..]
        .find("fn send_sidecar_command(")
        .map(|offset| stop_start + offset)
        .expect("source guard should find helper after stop_chat_runtime");
    let stop_body = &source[stop_start..send_sidecar_start];

    assert!(
        source.contains("fn stop_pcm_stream_in_background("),
        "realtime chat cleanup should define a dedicated background PCM stop helper"
    );
    assert!(
        dispose_body.contains("stop_pcm_stream_in_background"),
        "dispose should delegate PCM cleanup to the background helper"
    );
    assert!(
        stop_body.contains("stop_pcm_stream_in_background"),
        "stop_chat_runtime should delegate PCM cleanup to the background helper"
    );
    assert!(
        !dispose_body.contains("pcm_stream.stop()"),
        "dispose must not call pcm_stream.stop() inline on the foreground path"
    );
    assert!(
        !stop_body.contains("pcm_stream.stop()"),
        "stop_chat_runtime must not call pcm_stream.stop() inline on the foreground path"
    );
}
