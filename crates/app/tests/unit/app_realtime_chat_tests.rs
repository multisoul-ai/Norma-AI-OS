use super::app_realtime_tests_chat_support::{
    chunk, config, with_realtime_process_guard, RecordingAudioSink, RecordingPcmStream,
    RecordingSidecar,
};
use super::RealtimeControlState;
use crate::app::voice::control::VoiceVisual;
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeSessionId};

/// RTC-1: start chat path stores PCM stream, starts sidecar, sends start_session, then reaches Listening.
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = rt-chat-start
///   model = gpt-realtime-2
///   fake PCM stream stop count = 0 at start
///   fake sidecar emits session_started immediately after start_session
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_sidecar_and_pcm_for_tests(config, sidecar, pcm) → Connecting
///   2. fake sidecar records RealtimeCommand::StartSession
///   3. drain_sidecar_events consumes SessionStarted → Listening
///
/// 预期结果：
///   - 正断言：PCM stream 被 state 持有，sidecar 收到 start_session，label 到 Always Listening
///   - 负断言：启动阶段不应发送 stop_session，也不应提前停止 PCM stream
#[test]
fn test_realtime_chat_start_starts_pcm_sidecar_and_reaches_listening() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        let sidecar = RecordingSidecar::default();
        let pcm = RecordingPcmStream::default();

        state
            .start_shell_with_sidecar_and_pcm_for_tests(
                config("rt-chat-start"),
                Box::new(sidecar.clone()),
                Box::new(pcm.clone()),
            )
            .expect("valid chat start should enter Connecting and send start_session");

        assert!(
            state.has_pcm_stream_for_tests(),
            "chat start should store the active PCM stream handle"
        );
        assert!(
            sidecar
                .commands()
                .contains(&RealtimeCommand::StartSession(config("rt-chat-start"))),
            "chat start should send start_session to the sidecar"
        );
        assert!(
            !sidecar
                .commands()
                .iter()
                .any(|command| matches!(command, RealtimeCommand::StopSession { .. })),
            "chat start should not send stop_session before the user stops Always Listening"
        );

        state.drain_sidecar_events();

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "session_started should move the chat shell into Listening label"
        );
        assert_eq!(
            pcm.stop_calls(),
            0,
            "starting chat should not stop the PCM stream immediately"
        );
        state.stop_shell();
    });
}

/// RTC-2: forwarding one PCM chunk sends input_audio_chunk to the sidecar.
///
/// 数据构造（含关键数值的推导过程）：
///   PCM bytes = [0, 1, 2, 3]
///   byte length = 4；4 % 2 = 0，因此是合法 PCM16
///   sample_rate = 24_000Hz，channels = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar chat 并 drain 到 Listening
///   2. send_input_audio_chunk(chunk)
///   3. 读取 fake sidecar 记录的 commands
///
/// 预期结果：
///   - 正断言：commands 中出现精确的 InputAudioChunk(session_id=rt-chat-pcm, chunk)
///   - 负断言：普通 Listening 下发送音频不应先发送 interrupt
#[test]
fn test_realtime_chat_forwarding_pcm_chunk_sends_input_audio_chunk_to_sidecar() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        let sidecar = RecordingSidecar::default();
        state
            .start_shell_with_sidecar_for_tests(config("rt-chat-pcm"), Box::new(sidecar.clone()))
            .expect("valid sidecar test start should send start_session");
        state.drain_sidecar_events();

        let pcm = chunk(vec![0, 1, 2, 3]);
        let sent = state
            .send_input_audio_chunk(pcm.clone())
            .expect("sending valid PCM to fake sidecar should not fail");

        assert!(
            sent,
            "send_input_audio_chunk should report that an active sidecar received the chunk"
        );
        assert!(
            sidecar
                .commands()
                .contains(&RealtimeCommand::InputAudioChunk {
                    session_id: RealtimeSessionId::new("rt-chat-pcm"),
                    chunk: pcm,
                }),
            "forwarding PCM should send input_audio_chunk with the exact session and bytes"
        );
        assert!(
            !sidecar
                .commands()
                .iter()
                .any(|command| matches!(command, RealtimeCommand::Interrupt { .. })),
            "Listening-state PCM forwarding should not send interrupt before assistant is speaking"
        );
        state.stop_shell();
    });
}

/// RTC-3: assistant speaking state maps the bottom bar to Replying... and active visual.
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = rt-chat-speaking
///   event = AssistantAudioState { speaking: true }
///   fallback label = Voice；fallback visual = Idle
///
/// 执行过程（逐步说明系统如何处理）：
///   1. start_shell_with_config + mark_started → Listening
///   2. apply_sidecar_event_for_tests(assistant speaking true)
///   3. 读取 effective label / visual
///
/// 预期结果：
///   - 正断言：label 显示 Replying...，visual 是 Recording
///   - 负断言：assistant speaking 不应继续显示 Voice 或 Always Listening
#[test]
fn test_realtime_chat_assistant_speaking_state_maps_bottom_bar_to_replying() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_config(config("rt-chat-speaking"))
            .expect("valid config should enter Connecting");
        state.mark_started();

        state.apply_sidecar_event_for_tests(RealtimeEvent::AssistantAudioState {
            session_id: RealtimeSessionId::new("rt-chat-speaking"),
            speaking: true,
        });

        assert_eq!(
            state.effective_voice_label("Voice"),
            "Replying...",
            "assistant speaking should map the bottom label to Replying..."
        );
        assert_eq!(
            state.effective_voice_visual(VoiceVisual::Idle),
            VoiceVisual::Recording,
            "assistant speaking should use an active voice visual"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Voice",
            "assistant speaking must not fall back to idle Voice"
        );
        assert_ne!(
            state.effective_voice_label("Voice"),
            "Always Listening",
            "assistant speaking should distinguish reply playback from passive listening"
        );
        state.stop_shell();
    });
}

/// RTC-4: assistant_audio_chunk event writes the chunk to AssistantAudioSink.
///
/// 数据构造（含关键数值的推导过程）：
///   assistant bytes = [8, 9, 10, 11]
///   byte length = 4；4 % 2 = 0，因此是合法 PCM16
///   fake sink 初始 chunks.len = 0，stop_calls = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装 RecordingAudioSink
///   2. apply_sidecar_event_for_tests(AssistantAudioChunk)
///   3. 读取 fake sink 的 chunks 与 stop_calls
///
/// 预期结果：
///   - 正断言：sink 收到精确的 PCM chunk
///   - 负断言：普通音频 chunk 写入不应调用 stop
#[test]
fn test_realtime_chat_assistant_audio_chunk_writes_to_audio_sink() {
    with_realtime_process_guard(|| {
        let mut state = RealtimeControlState::default();
        state
            .start_shell_with_config(config("rt-chat-audio"))
            .expect("valid config should enter Connecting");
        state.mark_started();
        let sink = RecordingAudioSink::default();
        state.set_assistant_audio_sink_for_tests(Box::new(sink.clone()));

        let assistant_chunk = chunk(vec![8, 9, 10, 11]);
        state.apply_sidecar_event_for_tests(RealtimeEvent::AssistantAudioChunk {
            session_id: RealtimeSessionId::new("rt-chat-audio"),
            chunk: assistant_chunk.clone(),
        });

        assert_eq!(
            sink.chunks(),
            vec![assistant_chunk],
            "assistant_audio_chunk should be written to the assistant audio sink"
        );
        assert_eq!(
            sink.stop_calls(),
            0,
            "writing a normal assistant chunk should not stop playback"
        );
        state.stop_shell();
    });
}
