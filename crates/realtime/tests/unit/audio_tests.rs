use super::{PcmAudioChunk, PcmAudioError, PcmFormat};

fn assert_pcm_audio_chunk_traits<T>()
where
    T: Clone
        + std::fmt::Debug
        + Eq
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>,
{
}

/// A-1: PCM16 音频块校验接受有效输入并拒绝无效边界。
///
/// 数据构造（含关键数值的推导过程）：
///   valid_bytes = vec![0, 1, 2, 3]，PCM16 每个 sample = 2 bytes，4 % 2 = 0
///   zero_rate   = sample_rate_hz(0)，采样率必须 > 0
///   zero_chan   = channels(0)，声道数必须 > 0
///   odd_bytes   = vec![0]，PCM16 每个 sample = 2 bytes，1 % 2 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 24_000Hz / mono / 4 bytes PCM16 块 → 通过校验
///   2. 构造 0Hz / mono / 2 bytes PCM16 块 → 拒绝采样率
///   3. 构造 24_000Hz / 0 channel / 2 bytes PCM16 块 → 拒绝声道数
///   4. 构造 24_000Hz / mono / 1 byte PCM16 块 → 拒绝 PCM16 奇数字节长度
///
/// 预期结果：
///   - 正断言：有效块返回 Ok，且字段完整保留
///   - 正断言：三个无效输入分别返回 InvalidSampleRate / InvalidChannels / InvalidPcm16Length
///   - 负断言：有效块不能误报错误，三类错误不能互相混淆
#[test]
fn test_pcm16_validation_accepts_valid_chunk_and_rejects_invalid_boundaries() {
    assert_pcm_audio_chunk_traits::<PcmAudioChunk>();

    let valid = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3]);
    assert_eq!(
        valid,
        Ok(PcmAudioChunk {
            format: PcmFormat::Pcm16,
            sample_rate_hz: 24_000,
            channels: 1,
            bytes: vec![0, 1, 2, 3],
        }),
        "valid PCM16 mono data with even byte length should be accepted and preserved"
    );
    assert_ne!(
        valid,
        Err(PcmAudioError::InvalidPcm16Length),
        "valid PCM16 data must not be reported as an odd-length PCM16 payload"
    );

    let zero_sample_rate = PcmAudioChunk::new(PcmFormat::Pcm16, 0, 1, vec![0, 1]);
    assert_eq!(
        zero_sample_rate,
        Err(PcmAudioError::InvalidSampleRate),
        "sample_rate_hz=0 should be rejected as InvalidSampleRate"
    );
    assert_ne!(
        zero_sample_rate,
        Err(PcmAudioError::InvalidChannels),
        "sample_rate_hz=0 must not be confused with an invalid channel count"
    );

    let zero_channels = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 0, vec![0, 1]);
    assert_eq!(
        zero_channels,
        Err(PcmAudioError::InvalidChannels),
        "channels=0 should be rejected as InvalidChannels"
    );
    assert_ne!(
        zero_channels,
        Err(PcmAudioError::InvalidSampleRate),
        "channels=0 must not be confused with an invalid sample rate"
    );

    let odd_pcm16_length = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0]);
    assert_eq!(
        odd_pcm16_length,
        Err(PcmAudioError::InvalidPcm16Length),
        "PCM16 byte length 1 is odd, so it should be rejected as InvalidPcm16Length"
    );
    assert_ne!(
        odd_pcm16_length,
        Err(PcmAudioError::InvalidChannels),
        "odd PCM16 payload length must not be confused with an invalid channel count"
    );
}
