use super::{PcmAudioChunk, PcmAudioError, PcmFormat};

/// PCM-1: 16k/22.05k/24k mono PCM16 合法；其它采样率与奇数字节非法。
///
/// 数据构造（含关键数值的推导过程）：
///   asr_bytes   = vec![0, 1]，PCM16 每个 sample = 2 bytes，2 % 2 = 0，16_000Hz mono
///   tts_bytes   = vec![0, 1]，官方 Matcha 输出 22_050Hz mono
///   play_bytes  = vec![0, 1, 2, 3]，4 % 2 = 0，24_000Hz mono
///   bad_rate    = 48_000Hz mono，MVP 仅允许 16_000 / 22_050 / 24_000
///   stereo      = 16_000Hz / channels(2)，MVP 仅允许 mono
///   odd_bytes   = vec![0]，PCM16 每个 sample = 2 bytes，1 % 2 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 16_000Hz / mono / 2 bytes PCM16 块 → 通过校验（ASR/VAD 路径）
///   2. 构造 22_050Hz / mono / 2 bytes PCM16 块 → 通过校验（Matcha TTS）
///   3. 构造 24_000Hz / mono / 4 bytes PCM16 块 → 通过校验（播放 legacy 路径）
///   4. 构造 48_000Hz / mono / 2 bytes PCM16 块 → 拒绝采样率
///   5. 构造 16_000Hz / stereo / 2 bytes PCM16 块 → 拒绝声道数
///   6. 构造 16_000Hz / mono / 1 byte PCM16 块 → 拒绝 PCM16 奇数字节长度
///
/// 预期结果：
///   - 正断言：16k、22.05k 与 24k mono PCM16 偶数字节返回 Ok
///   - 负断言：其它采样率返回 InvalidSampleRate
///   - 负断言：非 mono 返回 InvalidChannels
///   - 负断言：奇数字节返回 InvalidPcm16Length
#[test]
fn test_pcm_accepts_supported_voice_rates_and_rejects_other_rates_and_odd_bytes() {
    assert!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 1]).is_ok(),
        "16_000Hz mono PCM16 even bytes must be accepted for ASR/VAD"
    );
    assert!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 22_050, 1, vec![0, 1]).is_ok(),
        "22_050Hz mono PCM16 must be accepted for official Matcha TTS output"
    );
    assert!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3]).is_ok(),
        "24_000Hz mono PCM16 must remain accepted for playback chunks"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 48_000, 1, vec![0, 1]),
        Err(PcmAudioError::InvalidSampleRate),
        "MVP must reject sample rates other than 16k/22.05k/24k"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 2, vec![0, 1]),
        Err(PcmAudioError::InvalidChannels),
        "MVP audio path is mono-only"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0]),
        Err(PcmAudioError::InvalidPcm16Length),
        "odd PCM16 byte length must be rejected"
    );
}
