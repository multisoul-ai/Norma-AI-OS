use super::FakeTtsClient;
use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tts::{TtsChunkSink, TtsClient, TtsError, TtsStreamSummary};

#[derive(Default)]
struct RecordingSink {
    chunks: Mutex<Vec<PcmAudioChunk>>,
    terminal: Mutex<Vec<Result<TtsStreamSummary, TtsError>>>,
    cancelled: AtomicBool,
}

impl TtsChunkSink for RecordingSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        self.chunks
            .lock()
            .expect("local recording sink lock should not be poisoned")
            .push(chunk);
        true
    }

    fn finish(&self, result: Result<TtsStreamSummary, TtsError>) {
        self.terminal
            .lock()
            .expect("local terminal sink lock should not be poisoned")
            .push(result);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// 本地 TTS 通过统一回复级 seam 顺序处理两个语义段并只结束一次。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate = 16,000 Hz
///   每段时长 = 100 ms = 16,000 / 10 = 1,600 frames
///   PCM16 bytes/段 = 1,600 × 2 = 3,200 bytes
///   text segments = 2，因此 chunks = 2、total bytes = 6,400
///
/// 执行过程：
///   1. start_stream 建立一个本地回复流。
///   2. 顺序 push 两个非空文本段。
///   3. finish_stream 收口一次。
///
/// 预期结果：
///   - 正断言：两个 PCM chunk 顺序交付，终态统计为 2/6,400。
///   - 负断言：push 期间不提前结束，finish 后不产生第二个终态。
#[test]
fn local_adapter_handles_two_segments_in_one_reply_stream() {
    let sink = Arc::new(RecordingSink::default());
    let mut client = FakeTtsClient::silent_pcm16_16k();

    client
        .start_stream(sink.clone())
        .expect("local reply stream should start");
    client
        .push_text("第一段。")
        .expect("first local segment should synthesize");
    client
        .push_text("第二段。")
        .expect("second local segment should synthesize");
    assert!(
        sink.terminal
            .lock()
            .expect("local terminal sink lock should not be poisoned")
            .is_empty(),
        "本地回复流在 finish_stream 前不得提前发送终态"
    );

    client
        .finish_stream()
        .expect("local reply stream should finish");

    let chunks = sink
        .chunks
        .lock()
        .expect("local recording sink lock should not be poisoned");
    assert_eq!(chunks.len(), 2, "两个语义段应产生两个有序 PCM chunk");
    assert_eq!(chunks[0].bytes.len(), 3_200, "第一段应为 100 ms PCM16");
    assert_eq!(chunks[1].bytes.len(), 3_200, "第二段应为 100 ms PCM16");
    let terminal = sink
        .terminal
        .lock()
        .expect("local terminal sink lock should not be poisoned");
    assert_eq!(terminal.len(), 1, "本地回复流必须只发送一次终态");
    let summary = terminal[0].as_ref().expect("本地回复流终态应成功");
    assert_eq!(summary.chunk_count(), 2, "终态应统计两个 PCM chunk");
    assert_eq!(summary.byte_count(), 6_400, "终态应统计 6,400 PCM bytes");
}
