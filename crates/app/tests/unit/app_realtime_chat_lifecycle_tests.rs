use super::app_realtime_tests_chat_support::{
    chunk, config, wait_until, with_realtime_process_guard, RecordingAudioSink, RecordingPcmStream,
    RecordingSidecar,
};
use super::RealtimeControlState;
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
///   4. 等待 fake sidecar stop helper 记录 stop
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
            || sidecar.stop_calls() == 1,
            "stop_shell should call sidecar stop helper before timeout",
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
