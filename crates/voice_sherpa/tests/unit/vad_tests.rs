use crate::samples::pcm16_chunk_to_f32_samples;
use audio::{PcmAudioChunk, PcmFormat};

/// VAD-UT-1: PCM16 静音块能量低于阈值时不应触发 SpeechStart。
///
/// 数据构造：
///   bytes = [0,0,0,0] → 能量 0
///   threshold = 0.01
///
/// 执行过程：
///   1. FakeEnergyVadClient::push_pcm
///
/// 预期结果：
///   - 负断言：不应产生任何 VAD 事件
#[test]
fn test_fake_energy_vad_silence_emits_no_events() {
    use crate::vad::FakeEnergyVadClient;
    use dialogue::{VadClient, VadEvent};

    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0, 0, 0])
        .expect("静音 PCM 块应通过校验");
    let mut vad = FakeEnergyVadClient::new(0.01);

    let events = vad.push_pcm(&chunk).expect("静音块 push_pcm 应成功");

    assert!(
        events.is_empty(),
        "静音块不应产生 VAD 事件，实际: {events:?}"
    );
    assert!(
        !events.contains(&VadEvent::SpeechStart),
        "静音块不应触发 SpeechStart"
    );
}

/// VAD-UT-2: 高能量块应先 SpeechStart 再 SpeechEnd。
///
/// 数据构造：
///   sample = i16::MAX → 能量远高于 threshold
///
/// 执行过程：
///   1. 推送高能量块 → SpeechStart
///   2. 推送静音块 → SpeechEnd
///
/// 预期结果：
///   - 正断言：两步事件序列正确
#[test]
fn test_fake_energy_vad_detects_start_and_end() {
    use crate::vad::FakeEnergyVadClient;
    use dialogue::{VadClient, VadEvent};

    let loud = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, i16::MAX.to_le_bytes().to_vec())
        .expect("高能量 PCM 块应通过校验");
    let silence =
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0]).expect("静音 PCM 块应通过校验");
    let mut vad = FakeEnergyVadClient::new(0.01);

    let start_events = vad.push_pcm(&loud).expect("高能量块应成功");
    assert_eq!(
        start_events,
        vec![VadEvent::SpeechStart],
        "高能量块应触发 SpeechStart"
    );

    let end_events = vad.push_pcm(&silence).expect("静音块应成功");
    assert_eq!(
        end_events,
        vec![VadEvent::SpeechEnd],
        "紧随其后的静音块应触发 SpeechEnd"
    );
}

/// VAD-UT-3: pcm16_chunk_to_f32_samples 应正确解码 PCM16。
#[test]
fn test_pcm16_chunk_to_f32_samples_decodes_le_bytes() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, i16::MAX.to_le_bytes().to_vec())
        .expect("PCM 块应通过校验");

    let samples = pcm16_chunk_to_f32_samples(&chunk).expect("解码应成功");
    assert_eq!(samples.len(), 1, "单样本块应解码为 1 个 f32");
    assert!(
        samples[0] > 0.99,
        "i16::MAX 应接近 +1.0，实际 {}",
        samples[0]
    );
}

/// VAD-UT-4: 真实 Silero VAD 在模型存在时可创建（无模型则忽略）。
#[test]
#[ignore = "requires silero VAD model assets on disk"]
fn test_sherpa_vad_client_creates_with_real_model() {
    let model_dir = crate::resolve_model_dir_for_test().expect("paraformer model dir 应可解析");
    crate::vad::SherpaVadClient::from_asr_model_dir(&model_dir)
        .expect("存在 silero VAD 模型时应能创建客户端");
}
