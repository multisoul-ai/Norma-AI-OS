use crate::samples::pcm16_chunk_to_f32_samples;
use audio::{PcmAudioChunk, PcmFormat};

/// SMP-1: 偶数字节 PCM16 块应解码为等量 f32 样本。
#[test]
fn test_pcm16_chunk_to_f32_samples_even_byte_length() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0, 1, 0, 255, 127])
        .expect("偶数字节 PCM 块应通过校验");

    let samples = pcm16_chunk_to_f32_samples(&chunk).expect("解码应成功");
    assert_eq!(samples.len(), 3, "6 字节 PCM16 应解码为 3 个 f32 样本");
}

/// SMP-2: 非 16kHz 块应被拒绝。
#[test]
fn test_pcm16_chunk_to_f32_samples_rejects_non_16khz() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 0])
        .expect("24kHz PCM 块构造应成功");

    assert!(
        pcm16_chunk_to_f32_samples(&chunk).is_err(),
        "VAD/ASR 路径应拒绝非 16kHz 块"
    );
}
