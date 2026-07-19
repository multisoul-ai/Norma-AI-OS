use super::{
    default_assistant_audio_sink, interrupt_assistant_playback, AssistantAudioSink,
    RecordingAssistantAudioSink,
};
use audio::{PcmAudioChunk, PcmFormat};
#[cfg(target_os = "macos")]
use dialogue::{
    handle_speaking_speech_start, DialoguePhase, DialogueSession, SpeakingSpeechSource, TtsClient,
};

#[cfg(target_os = "macos")]
fn installed_paraformer() -> asr::AvailableAsrModel {
    let catalog = asr::builtin_model_catalog();
    let id = asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID).expect("内置 Paraformer ID 应有效");
    let descriptor = catalog.find(&id).expect("模型库应包含 Paraformer");
    let model_root = asr::default_model_installation_root()
        .expect("真实语音测试需要可用的 Application Support 目录");
    asr::validate_installation(&model_root.join(id.as_str()), descriptor)
        .expect("真实语音测试要求通过新清单安装 Paraformer")
}

fn test_pcm_chunk() -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0, 0, 0])
        .expect("测试 PCM 块应通过校验")
}

/// PB-1: interrupt_assistant_playback 应调用 sink.stop。
///
/// 执行过程：
///   1. write_chunk 写入一块
///   2. interrupt_assistant_playback
///
/// 预期结果：
///   - 正断言：stopped == true
///   - 正断言：chunks_written == 1
#[test]
fn test_interrupt_assistant_playback_stops_sink() {
    let mut sink = RecordingAssistantAudioSink::new();
    sink.write_chunk(&test_pcm_chunk())
        .expect("Recording sink 应接受测试 chunk");
    assert_eq!(sink.chunks_written(), 1, "前置：应记录一次 write_chunk");
    assert!(!sink.stopped(), "前置：尚未 stop");

    interrupt_assistant_playback(&mut sink);

    assert!(sink.stopped(), "interrupt 应调用 stop");
    assert_eq!(sink.chunks_written(), 1, "interrupt 不应额外写入 chunk");
}

/// PB-2: sink 应接受 16k、22.05k 与 24k PCM16 mono 块。
#[test]
fn test_recording_sink_accepts_supported_voice_chunks() {
    let mut sink = RecordingAssistantAudioSink::new();
    let chunk_16k = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 1]).expect("16k chunk");
    let chunk_22050 =
        PcmAudioChunk::new(PcmFormat::Pcm16, 22_050, 1, vec![0, 1]).expect("22.05k chunk");
    let chunk_24k =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3]).expect("24k chunk");

    let duration_16k = sink.write_chunk(&chunk_16k).expect("16k 应可调度");
    let duration_22050 = sink.write_chunk(&chunk_22050).expect("22.05k 应可调度");
    let duration_24k = sink.write_chunk(&chunk_24k).expect("24k 应可调度");

    assert_eq!(
        sink.chunks_written(),
        3,
        "16k、22.05k 与 24k chunk 都应被接受"
    );
    assert!(duration_16k.as_nanos() > 0, "16k 调度时长应大于零");
    assert!(duration_22050.as_nanos() > 0, "22.05k 调度时长应大于零");
    assert!(duration_24k.as_nanos() > 0, "24k 调度时长应大于零");
}

/// PB-2b：播放启动和完成信号必须由 sink 显式交回，而不是由上层按 PCM 理论时长伪造。
///
/// 数据构造：Recording sink 与一块合法 16kHz mono PCM；启动和完成 channel 容量均为 1。
/// 执行过程：通过 write_chunk_with_completion 调度该 PCM，再立即读取启动和完成信号。
/// 预期结果：调度成功后必须各收到一个启动、完成信号；两者只对应本次实际 sink 调度。
#[test]
fn test_recording_sink_emits_explicit_playback_completion() {
    let mut sink = RecordingAssistantAudioSink::new();
    let (playback_started_tx, playback_started_rx) = async_channel::bounded(1);
    let (completion_tx, completion_rx) = async_channel::bounded(1);

    sink.write_chunk_with_completion(&test_pcm_chunk(), playback_started_tx, completion_tx)
        .expect("Recording sink 应能调度带完成回调的 PCM");

    playback_started_rx
        .try_recv()
        .expect("Recording sink 必须显式发出本次播放启动信号");
    completion_rx
        .try_recv()
        .expect("Recording sink 必须显式发出本次播放完成信号");
    assert_eq!(
        sink.chunks_written(),
        1,
        "完成信号不能掩盖或重复一次 PCM 调度"
    );
}

/// PB-3: macOS 默认 assistant sink 必须真正接受 PCM，而不是静默 Noop。
///
/// 数据构造：
///   100ms、22.05kHz、mono PCM16 静音块，采样率与官方 Matcha 输出一致。
///
/// 执行过程：
///   1. 创建 default_assistant_audio_sink；
///   2. 调用 write_chunk 调度音频。
///
/// 预期结果：
///   - 正断言：返回 Ok 且调度时长为 100ms；
///   - 负断言：不得返回 playback backend unavailable。
#[cfg(target_os = "macos")]
#[test]
fn test_macos_default_sink_schedules_matcha_pcm() {
    let frame_count = 22_050 / 10;
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 22_050, 1, vec![0; frame_count * 2])
        .expect("100ms Matcha PCM chunk 应合法");
    let mut sink = default_assistant_audio_sink();

    let duration = sink
        .write_chunk(&chunk)
        .expect("macOS 默认 sink 必须调度音频，不能是 Noop");

    assert_eq!(
        duration.as_millis(),
        100,
        "22.05kHz 的 2205 frames 应对应 100ms"
    );
    assert_ne!(duration.as_nanos(), 0, "默认 sink 不得返回零时长伪成功");
}

/// PB-4: Agent 完成回执的真实 Matcha TTS 必须能由 macOS 默认 sink 完整播放。
///
/// 数据构造：
///   文本 = `Raya, Done`（Agent 完成回执的精确 V1 短语）；
///   TTS = scripts/download-matcha-tts-model.sh 安装的中文真实模型；
///   sink = default_assistant_audio_sink（AVFoundation 后端）。
///
/// 执行过程：
///   1. 生产 SherpaTtsClient 合成真实 22050Hz PCM；
///   2. 默认 sink 调度该 PCM；
///   3. 等待音频时长，确保硬件有机会完成播放。
///
/// 预期结果：
///   - 正断言：中文文本合成 PCM 非空且为 22050Hz；
///   - 正断言：sink 返回的播放时长大于零；
///   - 负断言：不得返回 Kokoro 的 24000Hz 或 backend unavailable。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "plays audible speech; requires downloaded Matcha TTS model and macOS audio output"]
fn test_real_agent_done_utterance_plays_through_macos_default_sink() {
    let mut tts =
        voice_sherpa::SherpaTtsClient::from_env_or_default().expect("真实 Matcha TTS 模型应已安装");
    let chunk = tts
        .synthesize("Raya, Done")
        .expect("真实 Matcha TTS 应合成 Agent 完成回执 PCM");
    assert_eq!(
        chunk.sample_rate_hz, 22_050,
        "官方 Matcha 输出应保持 22050Hz"
    );
    assert_ne!(
        chunk.sample_rate_hz, 24_000,
        "默认播放链不得继续使用 24000Hz 的 Kokoro 模型"
    );
    assert!(!chunk.bytes.is_empty(), "真实 Matcha PCM 不应为空");

    let mut sink = default_assistant_audio_sink();
    let duration = sink
        .write_chunk(&chunk)
        .expect("AVFoundation 默认 sink 应调度真实 TTS");

    assert!(duration.as_millis() > 0, "真实 TTS 播放时长应大于零");
    std::thread::sleep(duration + std::time::Duration::from_millis(100));
}

/// PB-L3-2: assistant 扬声器回声即使被麦克风/VAD 捕获，也不得截断正在播放的回复。
///
/// 数据构造（含关键数值的推导过程）：
///   capture = 生产 AVAudioEngine 麦克风输入；VAD = 真实 Silero；
///   playback = 生产 Matcha 合成约数秒中文语音，再由 AVFoundation 默认 sink 播放；
///   监听窗口 = 完整播放时长 + 500ms，覆盖 AEC 收敛与 VAD 150ms 起说阈值。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保持 Continuous Listening pipeline 运行；
///   2. 通过设备扬声器播放 assistant TTS；
///   3. 将 pipeline 的原始 SpeechStart 按 UnclassifiedMicrophone 交给生产 speaking policy。
///
/// 预期结果：
///   - 正断言：TTS 完整调度、session 在完整监听窗口内保持 Speaking；
///   - 负断言：原始回声事件不得被编排层解释为已确认用户 barge-in。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "L3 hardware smoke: plays audible TTS and requires mic plus ASR/VAD/TTS models"]
fn test_assistant_playback_echo_is_ignored_until_reply_completes() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use voice_sherpa::{spawn_listening_pipeline_loop, ListeningPipelineEvent, SherpaProvider};

    let provider = SherpaProvider::new(installed_paraformer());
    let (pcm_stream, pcm_rx) = provider
        .start_pcm_stream(None)
        .expect("真实默认麦克风应可启动");
    let (event_tx, event_rx) = async_channel::bounded(64);
    let (stop_tx, stop_rx) = mpsc::channel();
    let pipeline_thread = spawn_listening_pipeline_loop(
        provider.available_model().clone(),
        pcm_rx,
        event_tx,
        stop_rx,
    )
    .expect("真实 VAD/ASR pipeline 应可启动");
    std::thread::sleep(Duration::from_millis(300));

    let mut tts =
        voice_sherpa::SherpaTtsClient::from_env_or_default().expect("真实 Matcha TTS 应已安装");
    let chunk = tts
        .synthesize("好的，我已经听清楚了，现在为你完整回答这个问题。")
        .expect("真实 Matcha 应合成 assistant 测试回复");
    while event_rx.try_recv().is_ok() {}
    let mut sink = default_assistant_audio_sink();
    let playback_duration = sink
        .write_chunk(&chunk)
        .expect("生产 AVFoundation sink 应调度完整 assistant TTS");
    let playback_started = Instant::now();
    let deadline = playback_started + playback_duration + Duration::from_millis(500);
    let mut speech_start_elapsed = None;
    let mut echo_partial = None;
    let mut echo_final = None;
    let mut session = DialogueSession::new();
    session.on_listening_started();
    session
        .on_utterance_final()
        .expect("测试 session 应进入 Thinking");
    session
        .on_reply_ready()
        .expect("测试 session 应进入 Speaking");
    let mut interrupted = false;
    while Instant::now() < deadline {
        match event_rx.try_recv() {
            Ok(ListeningPipelineEvent::SpeechStart) => {
                speech_start_elapsed = Some(playback_started.elapsed());
                interrupted |= handle_speaking_speech_start(
                    &mut session,
                    SpeakingSpeechSource::UnclassifiedMicrophone,
                );
            }
            Ok(ListeningPipelineEvent::PartialTranscript(text)) => echo_partial = Some(text),
            Ok(ListeningPipelineEvent::FinalTranscript(text)) => echo_final = Some(text),
            Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }

    sink.stop();
    let _ = stop_tx.send(());
    pcm_stream.cancel();
    pipeline_thread
        .join()
        .expect("真实 VAD/ASR pipeline thread 应正常退出");

    assert!(
        playback_duration > Duration::from_secs(1),
        "测试回复应足够长，能复现首词后被截断，实际: {playback_duration:?}"
    );
    assert!(
        !interrupted,
        "assistant 扬声器回声不得被编排层处理为 barge-in；raw_start={speech_start_elapsed:?}, partial={echo_partial:?}, final={echo_final:?}"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "即使底层 VAD 捕获设备回声，完整播放结束前也应保持 Speaking"
    );
}
