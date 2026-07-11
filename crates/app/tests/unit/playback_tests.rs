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
use llm::LlmClient;

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

/// PB-2b：播放完成信号必须由 sink 显式交回，而不是由上层按 PCM 理论时长伪造。
///
/// 数据构造：Recording sink 与一块合法 16kHz mono PCM；完成 channel 容量为 1。
/// 执行过程：通过 write_chunk_with_completion 调度该 PCM，再立即读取完成信号。
/// 预期结果：调度成功后必须收到一个完成信号；该信号只对应本次实际 sink 调度。
#[test]
fn test_recording_sink_emits_explicit_playback_completion() {
    let mut sink = RecordingAssistantAudioSink::new();
    let (completion_tx, completion_rx) = async_channel::bounded(1);

    sink.write_chunk_with_completion(&test_pcm_chunk(), completion_tx)
        .expect("Recording sink 应能调度带完成回调的 PCM");

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

/// PB-4: 生产 Matcha TTS 音频必须能由 macOS 默认 sink 完整调度播放。
///
/// 数据构造：
///   文本 =「多灵魂桌面本地语音播放验证完成」；
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
fn test_real_matcha_chinese_tts_plays_through_macos_default_sink() {
    let mut tts =
        voice_sherpa::SherpaTtsClient::from_env_or_default().expect("真实 Matcha TTS 模型应已安装");
    let chunk = tts
        .synthesize("多灵魂桌面本地语音播放验证完成")
        .expect("真实 Matcha TTS 应合成中文 PCM");
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

/// PB-L3-1: 真实语音对话必须贯通真实语音 fixture→ASR→Ollama→TTS→扬声器。
///
/// 数据构造（含关键数值的推导过程）：
///   输入 = Paraformer 官方 16kHz/mono/PCM16 `test_wavs/0.wav`；
///   chunk = 512 samples，与 Silero window 对齐；尾部静音 = 1 秒，超过 250ms endpoint；
///   Ollama/Matcha/播放均使用生产默认配置。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 官方真实语音逐块送入 Silero VAD 与 Paraformer 得到 final transcript；
///   2. 生产 OllamaLlmClient 携带 card tools 生成简短中文回复；
///   3. 生产 SherpaTtsClient 合成回复，AVFoundation sink 调度并等待播完。
///
/// 预期结果：
///   - 正断言：捕获非空 final、Ollama 返回非空文本、TTS PCM 非空且播放时长大于零；
///   - 负断言：不得停在识别完成而没有回复，也不得落入 Noop sink。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "L3 integration smoke: plays audible speech and requires Ollama plus ASR/VAD/TTS models"]
fn test_real_end_to_end_voice_conversation_replies_audibly() {
    use std::time::Duration;
    use voice_sherpa::{ListeningPipelineEvent, SherpaListeningPipeline};

    let model_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("app crate 应位于 workspace/crates/app")
        .join("assets/models/paraformer");
    let mut reader = hound::WavReader::open(model_dir.join("test_wavs/0.wav"))
        .expect("Paraformer 官方测试 WAV 应存在");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "E2E WAV 应为 16kHz");
    assert_eq!(spec.channels, 1, "E2E WAV 应为 mono");
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("E2E WAV 应完整解码");
    let mut pipeline =
        SherpaListeningPipeline::from_model_dir(&model_dir).expect("真实 VAD/ASR 应加载");
    let mut events = Vec::new();
    for samples in samples.chunks(512) {
        events.extend(
            pipeline
                .process_chunk(&pcm16_chunk(samples))
                .expect("真实语音 chunk 应通过 VAD/ASR"),
        );
    }
    for samples in vec![0_i16; 16_000].chunks(512) {
        events.extend(
            pipeline
                .process_chunk(&pcm16_chunk(samples))
                .expect("尾部静音应触发 final"),
        );
    }
    let transcript = events
        .into_iter()
        .find_map(|event| match event {
            ListeningPipelineEvent::FinalTranscript(text) if !text.trim().is_empty() => Some(text),
            _ => None,
        })
        .expect("真实 Silero/Paraformer 应产生非空 final transcript");
    assert!(!transcript.trim().is_empty(), "ASR final 不得为空");

    let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let model =
        std::env::var("MULTISOUL_DIALOGUE_MODEL").unwrap_or_else(|_| "qwen3.5:0.8b".to_string());
    llm::probe_ollama_model_ready(&endpoint, &model).expect("真实 Ollama 应运行且已安装对话模型");
    let mut client = llm::OllamaLlmClient::new(endpoint, model);
    let response = client
        .chat(&llm::ChatRequest {
            messages: vec![
                llm::ChatMessage {
                    role: llm::Role::System,
                    content: "你是 Norma 语音助手。只用一句简短中文回答，不要使用表情符号。".into(),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
                llm::ChatMessage {
                    role: llm::Role::User,
                    content: transcript.clone(),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: llm::card_tool_specs(),
        })
        .expect("真实 Ollama 应为 ASR final 生成回复");
    assert!(
        !response.assistant_text.trim().is_empty(),
        "识别完成后 Ollama 必须返回可播报文本，实际: {response:?}"
    );

    let mut tts =
        voice_sherpa::SherpaTtsClient::from_env_or_default().expect("真实 Matcha TTS 应已安装");
    let chunk = tts
        .synthesize(&response.assistant_text)
        .expect("真实 Matcha TTS 应合成 Ollama 回复");
    assert!(
        !chunk.bytes.is_empty(),
        "Ollama 回复合成的 TTS PCM 不得为空"
    );
    let mut sink = default_assistant_audio_sink();
    let duration = sink
        .write_chunk(&chunk)
        .expect("生产 AVFoundation sink 应调度回复音频");
    assert!(duration.as_nanos() > 0, "回复播放时长必须大于零");
    eprintln!(
        "real voice conversation: transcript={transcript:?}, reply={:?}, playback={duration:?}",
        response.assistant_text
    );
    std::thread::sleep(duration + Duration::from_millis(100));
}

#[cfg(target_os = "macos")]
fn pcm16_chunk(samples: &[i16]) -> PcmAudioChunk {
    let bytes = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
        .expect("16kHz mono PCM16 fixture chunk 应合法")
}

/// PB-L3-2: assistant 扬声器回声即使被麦克风/VAD 捕获，也不得截断正在播放的回复。
///
/// 数据构造（含关键数值的推导过程）：
///   capture = 生产 AVAudioEngine 麦克风输入；VAD = 真实 Silero；
///   playback = 生产 Matcha 合成约数秒中文语音，再由 AVFoundation 默认 sink 播放；
///   监听窗口 = 完整播放时长 + 500ms，覆盖 AEC 收敛与 VAD 150ms 起说阈值。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保持 Always Listening pipeline 运行；
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

    let provider = SherpaProvider::from_env_or_default().expect("真实 Paraformer 模型应已安装");
    let model_dir = provider.model_dir().to_path_buf();
    let (pcm_stream, pcm_rx) = provider
        .start_pcm_stream(None)
        .expect("真实默认麦克风应可启动");
    let (event_tx, event_rx) = async_channel::bounded(64);
    let (stop_tx, stop_rx) = mpsc::channel();
    let pipeline_thread = spawn_listening_pipeline_loop(model_dir, pcm_rx, event_tx, stop_rx)
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
