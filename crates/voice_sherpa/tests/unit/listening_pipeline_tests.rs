use super::{
    apply_final_transcript_to_session, spawn_listening_pipeline_loop, ListeningPipelineEvent,
    SherpaListeningPipeline,
};
use crate::SpeechError;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{DialoguePhase, DialogueSession};
use std::path::PathBuf;
use std::sync::mpsc;

/// LP-UT-1: ASR 安装完整但缺失真实 VAD 时，启动入口必须同步返回可见错误。
///
/// 数据构造：
///   model_dir = 临时受管目录，含 3 个 ASR 占位文件与匹配清单，但不含 Silero VAD；
///   PCM 与事件通道均为空，确保失败只来自模型初始化。
///
/// 执行过程：
///   1. 领域校验先把完整 ASR 占位安装转换成 AvailableAsrModel；
///   2. 调用公开的 spawn_listening_pipeline_loop；
///   3. 检查启动结果，不等待后台线程或依赖麦克风。
///
/// 预期结果：
///   - 正断言：返回 SpeechError::ModelNotFound；
///   - 负断言：不得返回 Ok(JoinHandle)，否则 app 会误进入 Listening 后静默失效。
#[test]
fn test_listening_pipeline_start_reports_missing_models_before_spawning() {
    let model_dir = tempfile::tempdir().expect("应能创建受管 ASR 占位安装目录");
    let catalog = asr::builtin_model_catalog();
    let descriptor = catalog.entries().first().expect("模型库应包含 Paraformer");
    for file in descriptor.artifact().required_files() {
        std::fs::write(model_dir.path().join(file), [1_u8])
            .expect("应写入 ASR 必要占位文件，让失败只来自 VAD");
    }
    asr::InstallationManifest::for_descriptor(descriptor)
        .write_to(model_dir.path())
        .expect("应写入测试安装清单");
    let available = asr::validate_installation(model_dir.path(), descriptor)
        .expect("ASR 占位安装应通过领域校验");
    let (_pcm_tx, pcm_rx) = async_channel::bounded(1);
    let (event_tx, _event_rx) = async_channel::bounded(1);
    let (_stop_tx, stop_rx) = mpsc::channel();

    let result = spawn_listening_pipeline_loop(available, pcm_rx, event_tx, stop_rx);

    assert!(
        matches!(&result, Err(SpeechError::ModelNotFound(_))),
        "ASR 完整但缺失 Silero VAD 时应在启动边界返回 ModelNotFound，实际结果: {result:?}"
    );
    assert!(
        result.is_err(),
        "模型初始化失败时不得返回后台线程句柄并静默退出"
    );
}

/// LP-UT-2: 官方 16kHz 测试语音必须走通真实 VAD→流式 ASR→DialogueSession。
///
/// 数据构造（含关键数值的推导过程）：
///   WAV = assets/models/paraformer/test_wavs/0.wav，16kHz/mono/PCM16；
///   chunk = 512 samples = 1024 bytes，与 Silero VAD window_size 对齐；
///   尾部静音 = 16_000 samples = 1 秒，用于超过 0.25 秒 endpoint 阈值。
///
/// 执行过程：
///   1. 用真实 Paraformer + Silero 构造 SherpaListeningPipeline；
///   2. 逐块送入 WAV，再送入 1 秒静音；
///   3. 收集 SpeechStart、partial 与 FinalTranscript；
///   4. 将 final 应用到已进入 Listening 的 DialogueSession。
///
/// 预期结果：
///   - 正断言：至少出现一次 SpeechStart 与非空 FinalTranscript；
///   - 正断言：DialogueSession 从 Listening 进入 Thinking 并保存 transcript；
///   - 负断言：不得只产生 partial 后永远没有 final。
#[test]
#[ignore = "requires downloaded Paraformer and Silero models; run model download scripts first"]
fn test_real_wav_reaches_final_transcript_and_thinking() {
    let events = real_wav_events_for_model(asr::PARAFORMER_MODEL_ID);

    assert!(
        events.contains(&ListeningPipelineEvent::SpeechStart),
        "真实语音应触发 SpeechStart，实际事件数: {}",
        events.len()
    );
    let final_text = events.iter().find_map(|event| match event {
        ListeningPipelineEvent::FinalTranscript(text) if !text.trim().is_empty() => {
            Some(text.clone())
        }
        _ => None,
    });
    assert!(
        final_text.is_some(),
        "真实语音必须产生非空 final；不得只有 partial，事件: {events:?}"
    );

    let final_text = final_text.expect("上方断言已确认 final 存在");
    let mut session = DialogueSession::new();
    session.on_listening_started();
    apply_final_transcript_to_session(&mut session, &final_text)
        .expect("真实 final 应能推进 DialogueSession");

    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "真实 final 后会话必须进入 Thinking"
    );
    assert_eq!(
        session.pending_transcript(),
        Some(final_text.as_str()),
        "Thinking 前必须保留真实 transcript"
    );
}

/// LP-UT-3：Zipformer 官方工件必须通过统一 Transducer factory 产生 partial 与 final。
///
/// 数据构造（含关键数值的推导过程）：
///   model ID = zipformer-streaming-zh-int8-2025-06-30（固定官方工件）
///   WAV = assets/models/paraformer/test_wavs/0.wav，16kHz/mono/PCM16
///   chunk = 512 samples = 1024 bytes
///   尾部静音 = 16_000 samples = 1 秒
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Application Support 受管目录校验 Zipformer 安装
///   2. 统一 factory 构造 Online Transducer pipeline
///   3. 逐块输入真实 WAV 与 1 秒静音，收集事件
///
/// 预期结果：
///   - 正断言：Zipformer 至少产生一个非空 partial 与一个非空 final
///   - 负断言：不得只触发 SpeechStart 而没有可提交文本
#[test]
#[ignore = "requires downloaded Zipformer, Silero VAD and official test WAV assets"]
fn test_zipformer_real_wav_produces_partial_and_final_transcripts() {
    let events = real_wav_events_for_model(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID);
    let has_partial = events.iter().any(|event| {
        matches!(event, ListeningPipelineEvent::PartialTranscript(text) if !text.trim().is_empty())
    });
    let has_final = events.iter().any(|event| {
        matches!(event, ListeningPipelineEvent::FinalTranscript(text) if !text.trim().is_empty())
    });

    assert!(
        has_partial,
        "Zipformer 真实流式识别必须产生非空 partial，实际事件: {events:?}"
    );
    assert!(
        has_final,
        "Zipformer 真实流式识别必须产生非空 final，不能停在 SpeechStart/partial"
    );
    assert!(
        events.contains(&ListeningPipelineEvent::SpeechStart),
        "Zipformer 真实语音必须先触发 SpeechStart"
    );
}

fn real_wav_events_for_model(model_id: &str) -> Vec<ListeningPipelineEvent> {
    let wav_path = repo_root().join("assets/models/paraformer/test_wavs/0.wav");
    let mut reader = hound::WavReader::open(&wav_path).expect("官方测试 WAV 应可读取");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "测试 WAV 必须是 16kHz");
    assert_eq!(spec.channels, 1, "测试 WAV 必须是 mono");
    assert_eq!(spec.bits_per_sample, 16, "测试 WAV 必须是 PCM16");
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("测试 WAV samples 应完整解码");
    let catalog = asr::builtin_model_catalog();
    let id = asr::AsrModelId::parse(model_id).expect("内置真实模型 ID 应有效");
    let descriptor = catalog.find(&id).expect("模型库应包含真实测试模型");
    let managed_root =
        asr::default_model_installation_root().expect("真实测试需要 Application Support 模型根");
    let available = asr::validate_installation(&managed_root.join(id.as_str()), descriptor)
        .expect("真实测试要求模型已通过新架构安装");
    let mut pipeline = SherpaListeningPipeline::from_available_model(&available)
        .expect("真实模型应能创建 pipeline");
    let mut events = Vec::new();
    for sample_chunk in samples.chunks(512) {
        events.extend(
            pipeline
                .process_chunk(&pcm_chunk(sample_chunk))
                .expect("真实语音 chunk 应能通过 VAD/ASR"),
        );
    }
    for silence_chunk in vec![0_i16; 16_000].chunks(512) {
        events.extend(
            pipeline
                .process_chunk(&pcm_chunk(silence_chunk))
                .expect("尾部静音应能完成 endpoint"),
        );
    }
    events
}

fn pcm_chunk(samples: &[i16]) -> PcmAudioChunk {
    let bytes = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes).expect("16kHz mono PCM16 chunk 应合法")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("voice_sherpa 应位于 crates/voice_sherpa")
        .to_path_buf()
}
