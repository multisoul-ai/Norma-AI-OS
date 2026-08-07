use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("test log output mutex should not be poisoned")
            .write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn pcm_chunk(chunk_sequence: u64) -> QueuedPlaybackChunk {
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 32_000, 1, vec![0; 2_048])
        .expect("2,048-byte PCM16 mono fixture should be valid");
    let mut chunk = QueuedPlaybackChunk::from_pcm(pcm);
    chunk.tts_request_sequence = 7;
    chunk.chunk_sequence = chunk_sequence;
    chunk
}

/// LOG-TTS-1：默认 INFO 日志只记录每次 TTS 请求的首个 PCM 块，不按后续块持续写盘。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM chunk bytes     = 2,048
///   sample frames       = 2,048 / 2 bytes per PCM16 sample = 1,024
///   emitted chunks      = 253（真实日志中一次请求的 chunk_sequence 为 0..252）
///   suppressed chunks   = 253 - 1 first chunk = 252
///   expected INFO lines = 1（首块摘要 1 + 后续逐块日志 0）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装仅接收 INFO 及以上事件的内存 subscriber。
///   2. 发送 chunk_sequence=0 的首块事件，应保留一次流首延迟摘要。
///   3. 继续发送 chunk_sequence=1..252 的后续块事件，不应进入默认持久化级别。
///   4. 读取 subscriber 输出并统计 PCM received 事件。
///
/// 预期结果：
///   - 正断言：INFO 输出恰好包含一条 PCM received，且属于 chunk_sequence=0。
///   - 负断言：INFO 输出不包含末块 chunk_sequence=252，避免每块触发一次磁盘写入。
#[test]
fn default_info_logging_records_only_first_pcm_chunk() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer_output = Arc::clone(&output);
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .with_writer(move || SharedWriter(Arc::clone(&writer_output)))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        // Real request replay: chunk_sequence 0..252 means 253 emitted PCM chunks.
        for chunk_sequence in 0..253 {
            pcm_received(
                11,
                &pcm_chunk(chunk_sequence),
                Duration::from_millis(24 + chunk_sequence * 32),
            );
        }
    });

    let output = String::from_utf8(
        output
            .lock()
            .expect("test log output mutex should not be poisoned")
            .clone(),
    )
    .expect("tracing formatter should emit UTF-8 text");
    assert_eq!(
        output.matches("[TTS-TRACE] TTS PCM chunk received").count(),
        1,
        "默认 INFO 输出应只保留首个 PCM 块摘要，而不是为 253 个块各写一行"
    );
    assert!(
        output.contains("chunk_sequence=0"),
        "默认 INFO 输出必须包含 chunk_sequence=0 的首块摘要"
    );
    assert!(
        !output.contains("chunk_sequence=252"),
        "默认 INFO 输出不得包含后续 PCM 块，否则仍会按块写盘"
    );
}
