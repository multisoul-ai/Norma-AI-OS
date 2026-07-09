use crate::FakeTtsClient;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{DialogueError, TtsClient};

/// TTS-1: 空文本合成应返回 DialogueError。
///
/// 执行过程：
///   1. FakeTtsClient::synthesize("")
///
/// 预期结果：
///   - 负断言：返回 Err，消息包含 empty
#[test]
fn test_tts_empty_text_returns_error() {
    let mut tts = FakeTtsClient::silent_pcm16_16k();

    assert_eq!(
        tts.synthesize(""),
        Err(DialogueError::Message("empty TTS text".to_string())),
        "空文本不应合成"
    );
    assert_eq!(
        tts.synthesize("   "),
        Err(DialogueError::Message("empty TTS text".to_string())),
        "仅空白文本不应合成"
    );
}

/// TTS-2: 非空文本应返回合法 PCM16 mono 块（16k 或 24k）。
///
/// 预期结果：
///   - 正断言：sample_rate_hz 为 16_000 或 24_000
///   - 正断言：bytes 非空且长度为偶数
#[test]
fn test_fake_tts_returns_valid_pcm_chunk() {
    let mut tts_16k = FakeTtsClient::silent_pcm16_16k();
    let chunk_16k = tts_16k
        .synthesize("你好")
        .expect("非空文本应成功合成 16k 块");
    assert_valid_pcm_chunk(&chunk_16k, 16_000, "16k fake TTS");

    let mut tts_24k = FakeTtsClient::silent_pcm16_24k();
    let chunk_24k = tts_24k
        .synthesize("你好")
        .expect("非空文本应成功合成 24k 块");
    assert_valid_pcm_chunk(&chunk_24k, 24_000, "24k fake TTS");
}

fn assert_valid_pcm_chunk(chunk: &PcmAudioChunk, expected_rate: u32, label: &str) {
    assert_eq!(chunk.format, PcmFormat::Pcm16, "{label} 应为 PCM16");
    assert_eq!(chunk.sample_rate_hz, expected_rate, "{label} 采样率应匹配");
    assert_eq!(chunk.channels, 1, "{label} 应为 mono");
    assert!(!chunk.bytes.is_empty(), "{label} bytes 不应为空");
    assert_eq!(chunk.bytes.len() % 2, 0, "{label} PCM16 字节数应为偶数");
}
