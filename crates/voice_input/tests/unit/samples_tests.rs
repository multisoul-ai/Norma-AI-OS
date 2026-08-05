use crate::samples::pcm16_chunk_to_f32_samples;
use audio::{PcmAudioChunk, PcmFormat};

/// SMP-1：偶数字节 PCM16 块应按小端序解码为等量 f32 样本。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM bytes = 6 bytes
///   sample width = 2 bytes
///   sample count = 6 / 2 = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 16kHz、mono、PCM16 块
///   2. 将三个小端序样本转换为 f32
///
/// 预期结果：
///   - 正断言：恰好产生 3 个样本，最后一个接近 +1.0
///   - 负断言：第一个零样本不得被解码成非零值
#[test]
fn test_pcm16_chunk_to_f32_samples_even_byte_length() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0, 1, 0, 255, 127])
        .expect("偶数字节 PCM 块应通过校验");

    let samples = pcm16_chunk_to_f32_samples(&chunk).expect("解码应成功");
    assert_eq!(samples.len(), 3, "6 字节 PCM16 应解码为 3 个 f32 样本");
    assert_eq!(samples[0], 0.0, "零值 PCM16 样本不得被解码成非零值");
    assert!(samples[2] > 0.99, "i16::MAX 应被解码为接近 +1.0 的样本");
}

/// SMP-2：非 16kHz 块应在共享采样边界被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate = 24,000 Hz（目标 16,000 Hz 的 1.5 倍）
///   PCM bytes = 2 bytes = 1 sample
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造格式合法但采样率不匹配的 PCM 块
///   2. 调用统一 PCM16→f32 转换入口
///
/// 预期结果：
///   - 正断言：返回明确错误
///   - 负断言：不得产生任何可交给 VAD/ASR 的样本
#[test]
fn test_pcm16_chunk_to_f32_samples_rejects_non_16khz() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 0])
        .expect("24kHz PCM 块构造应成功");

    let result = pcm16_chunk_to_f32_samples(&chunk);
    assert!(result.is_err(), "VAD/ASR 路径应拒绝非 16kHz 块");
    assert!(result.ok().is_none(), "非 16kHz 输入不得产生样本向量");
}
