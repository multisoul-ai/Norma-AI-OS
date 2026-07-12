use super::{
    apply_final_transcript_to_session, spawn_listening_pipeline_loop, ListeningPipelineEvent,
    SherpaListeningPipeline,
};
use crate::SpeechError;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{DialoguePhase, DialogueSession};
use std::path::PathBuf;
use std::sync::mpsc;

/// LP-UT-1: 缺失真实 VAD/ASR 模型时，启动入口必须同步返回可见错误。
///
/// 数据构造：
///   model_dir = 空临时目录，不含 encoder/decoder/tokens 或 Silero VAD；
///   PCM 与事件通道均为空，确保失败只来自模型初始化。
///
/// 执行过程：
///   1. 调用公开的 spawn_listening_pipeline_loop；
///   2. 检查启动结果，不等待后台线程或依赖麦克风。
///
/// 预期结果：
///   - 正断言：返回 SpeechError::ModelNotFound；
///   - 负断言：不得返回 Ok(JoinHandle)，否则 app 会误进入 Listening 后静默失效。
#[test]
fn test_listening_pipeline_start_reports_missing_models_before_spawning() {
    let model_dir = tempfile::tempdir().expect("应能创建空模型临时目录");
    let (_pcm_tx, pcm_rx) = async_channel::bounded(1);
    let (event_tx, _event_rx) = async_channel::bounded(1);
    let (_stop_tx, stop_rx) = mpsc::channel();

    let result =
        spawn_listening_pipeline_loop(model_dir.path().to_path_buf(), pcm_rx, event_tx, stop_rx);

    assert!(
        matches!(&result, Err(SpeechError::ModelNotFound(_))),
        "空模型目录应在启动边界返回 ModelNotFound，实际结果: {result:?}"
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
    let model_dir = repo_root().join("assets/models/paraformer");
    let wav_path = model_dir.join("test_wavs/0.wav");
    let mut reader = hound::WavReader::open(&wav_path).expect("官方测试 WAV 应可读取");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "测试 WAV 必须是 16kHz");
    assert_eq!(spec.channels, 1, "测试 WAV 必须是 mono");
    assert_eq!(spec.bits_per_sample, 16, "测试 WAV 必须是 PCM16");

    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("测试 WAV samples 应完整解码");
    let mut pipeline =
        SherpaListeningPipeline::from_model_dir(&model_dir).expect("真实模型应能创建 pipeline");
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
