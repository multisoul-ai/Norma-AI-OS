use super::{interrupt_assistant_playback, AssistantAudioSink, RecordingAssistantAudioSink};
use audio::{PcmAudioChunk, PcmFormat};

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
    sink.write_chunk(&test_pcm_chunk());
    assert_eq!(sink.chunks_written(), 1, "前置：应记录一次 write_chunk");
    assert!(!sink.stopped(), "前置：尚未 stop");

    interrupt_assistant_playback(&mut sink);

    assert!(sink.stopped(), "interrupt 应调用 stop");
    assert_eq!(sink.chunks_written(), 1, "interrupt 不应额外写入 chunk");
}

/// PB-2: sink 应接受 16k 与 24k PCM16 mono 块。
#[test]
fn test_recording_sink_accepts_16k_and_24k_chunks() {
    let mut sink = RecordingAssistantAudioSink::new();
    let chunk_16k = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 1]).expect("16k chunk");
    let chunk_24k =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3]).expect("24k chunk");

    sink.write_chunk(&chunk_16k);
    sink.write_chunk(&chunk_24k);

    assert_eq!(sink.chunks_written(), 2, "16k 与 24k chunk 都应被接受");
}
