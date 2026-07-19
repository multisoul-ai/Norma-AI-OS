use crate::samples::pcm16_chunk_to_f32_samples;
use audio::{PcmAudioChunk, PcmFormat};

/// VAD-UT-1：PCM16 静音块能量低于阈值时不应触发 SpeechStart。
///
/// 数据构造（含关键数值的推导过程）：
///   bytes = [0,0,0,0] → 能量 0
///   threshold = 0.01
///
/// 执行过程（逐步说明系统如何处理）：
///   1. FakeEnergyVadClient::push_pcm
///   2. 随后推送高能量块，证明 VAD 仍能从静音状态进入 SpeechStart
///
/// 预期结果：
///   - 正断言：随后高能量块应产生唯一 SpeechStart
///   - 负断言：不应产生任何 VAD 事件
#[test]
fn test_fake_energy_vad_silence_emits_no_events() {
    use crate::vad::FakeEnergyVadClient;
    use crate::{VadClient, VadEvent};

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

    let loud = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, i16::MAX.to_le_bytes().to_vec())
        .expect("高能量 PCM 块应通过校验");
    let loud_events = vad.push_pcm(&loud).expect("静音后的高能量块应成功");
    assert_eq!(
        loud_events,
        vec![VadEvent::SpeechStart],
        "VAD 应保持可用并在后续高能量块产生唯一 SpeechStart"
    );
}

/// VAD-UT-2：高能量块应先 SpeechStart 再 SpeechEnd。
///
/// 数据构造（含关键数值的推导过程）：
///   sample = i16::MAX → 能量远高于 threshold
///   silence = 0 → 能量低于 threshold
///   threshold = 0.01
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 推送高能量块 → SpeechStart
///   2. 推送静音块 → SpeechEnd
///
/// 预期结果：
///   - 正断言：第一步恰好为 SpeechStart，第二步恰好为 SpeechEnd
///   - 负断言：两个事件不得颠倒或在同一步重复出现
#[test]
fn test_fake_energy_vad_detects_start_and_end() {
    use crate::vad::FakeEnergyVadClient;
    use crate::{VadClient, VadEvent};

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

/// VAD-UT-3：pcm16_chunk_to_f32_samples 应正确解码单个 PCM16 峰值。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM bytes = 2 bytes
///   sample width = 2 bytes
///   sample count = 2 / 2 = 1
///   sample = i16::MAX → 归一化值接近 +1.0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个小端序正峰值样本
///   2. 通过共享转换函数解码
///
/// 预期结果：
///   - 正断言：只产生一个大于 0.99 的样本
///   - 负断言：正峰值不得被解码为零或负值
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
    assert!(samples[0] > 0.0, "正峰值 PCM16 不得解码为零或负值");
}

/// VAD-UT-4：真实 Silero VAD 在模型存在时应创建成功。
///
/// 数据构造（含关键数值的推导过程）：
///   model roots = 1（由测试模型解析器定位）
///   PCM inputs = 0（本测试只验构造边界）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析受管模型目录
///   2. 从共享 Silero 工件创建 VAD 客户端
///
/// 预期结果：
///   - 正断言：模型存在时客户端构造成功
///   - 负断言：不得把有效共享模型误报为缺失
#[test]
#[ignore = "requires silero VAD model assets on disk"]
fn test_sherpa_vad_client_creates_with_real_model() {
    let model_dir = crate::resolve_model_dir_for_test().expect("paraformer model dir 应可解析");
    let result = crate::vad::SherpaVadClient::from_asr_model_dir(&model_dir);

    assert!(
        result.is_ok(),
        "存在 silero VAD 模型时应能创建客户端，实际错误: {:?}",
        result.err()
    );
    assert!(
        !matches!(
            crate::vad::SherpaVadClient::from_asr_model_dir(&model_dir),
            Err(crate::VoiceInputError::ModelNotFound(_))
        ),
        "有效共享模型不得被误报为 ModelNotFound"
    );
}
