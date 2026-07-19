use super::{MiniMaxProvider, MiniMaxTtsConfig};
use crate::{
    TtsChunkSink, TtsClient, TtsError, TtsModelId, TtsStreamSummary, TtsVoiceOrigin,
    MINIMAX_SPEECH_TURBO_MODEL_ID,
};
use audio::PcmAudioChunk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct LiveRecordingSink {
    chunks: Mutex<Vec<PcmAudioChunk>>,
    first_pcm_at: Mutex<Option<Instant>>,
    terminal: Mutex<Option<Result<TtsStreamSummary, TtsError>>>,
    cancelled: AtomicBool,
}

impl TtsChunkSink for LiveRecordingSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        let mut first_pcm_at = self
            .first_pcm_at
            .lock()
            .expect("live smoke first-PCM lock should not be poisoned");
        first_pcm_at.get_or_insert_with(Instant::now);
        self.chunks
            .lock()
            .expect("live smoke PCM lock should not be poisoned")
            .push(chunk);
        true
    }

    fn finish(&self, result: Result<TtsStreamSummary, TtsError>) {
        *self
            .terminal
            .lock()
            .expect("live smoke terminal lock should not be poisoned") = Some(result);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// LIVE-MINIMAX-1：真实 MiniMax 账户必须在一个 WebSocket task 中增量返回合法 PCM。
///
/// 数据构造（含关键数值的推导过程）：
///   environment credentials = 1（`MINIMAX_API_KEY`，缺失则显式跳过）
///   text segments = 2（首段 + 尾段）
///   WebSocket tasks = 1（一个 client start/finish 生命周期）
///   minimum PCM chunks before finish = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从环境读取密钥，查询账户 Voice 目录并选第一个 System Voice。
///   2. 用 Turbo 建立一个流，依次 push 两个中文语义段。
///   3. 在发送 task_finish 前读取 PCM 数量，再完成流并读取安全终态统计。
///
/// 预期结果：
///   - 正断言：finish 前至少已收到一个 32kHz mono、偶数字节 PCM chunk，终态成功。
///   - 正断言：完成统计包含非零 chunk/byte，便于建立真实 TTFA 基线。
///   - 负断言：不得把无 PCM 的最终响应误报成流式成功，也不得打印 API Key/Voice ID。
#[test]
#[ignore = "requires an explicit MINIMAX_API_KEY and invokes the user's MiniMax account"]
fn live_minimax_stream_emits_pcm_before_task_finish() {
    let Ok(api_key) = std::env::var("MINIMAX_API_KEY") else {
        eprintln!("MINIMAX_API_KEY is absent; live MiniMax smoke skipped explicitly");
        return;
    };
    let total_started = Instant::now();
    let provider = MiniMaxProvider::new(api_key).expect("environment credential must be non-empty");
    let connect_started = Instant::now();
    let catalog = provider
        .query_voices()
        .expect("supplied credential must return an authenticated Voice catalog");
    let catalog_elapsed = connect_started.elapsed();
    let voice_id = catalog
        .voices()
        .iter()
        .find(|voice| voice.origin() == TtsVoiceOrigin::System)
        .expect("live account must expose at least one System Voice for safe smoke coverage")
        .id()
        .clone();
    let config = MiniMaxTtsConfig::new(
        TtsModelId::parse(MINIMAX_SPEECH_TURBO_MODEL_ID)
            .expect("curated Turbo model ID must parse"),
        voice_id,
    )
    .expect("live smoke config must use the supported Turbo model");
    let sink = Arc::new(LiveRecordingSink::default());
    let task_started = Instant::now();
    let mut client = provider.tts_client(config);

    client
        .start_stream(sink.clone())
        .expect("live WebSocket must reach task_started");
    let task_start_elapsed = task_started.elapsed();
    client
        .push_text("这是第一段流式语音。")
        .expect("first live segment must be accepted");
    client
        .push_text("这是同一个任务中的第二段。")
        .expect("second live segment must share the active task");

    let chunks_before_finish = sink
        .chunks
        .lock()
        .expect("live smoke PCM lock should not be poisoned")
        .clone();
    assert!(
        !chunks_before_finish.is_empty(),
        "task_finish 前必须至少收到一个 PCM chunk，才能证明是真增量输出"
    );
    assert_eq!(
        chunks_before_finish[0].sample_rate_hz, 32_000,
        "MiniMax live PCM 必须为请求的 32 kHz"
    );
    assert_eq!(
        chunks_before_finish[0].channels, 1,
        "MiniMax live PCM 必须为 mono"
    );
    assert!(
        !chunks_before_finish[0].bytes.is_empty()
            && chunks_before_finish[0].bytes.len().is_multiple_of(2),
        "首个 live PCM chunk 必须非空且满足 PCM16 偶数字节对齐"
    );
    client
        .finish_stream()
        .expect("live task_finish must complete successfully");
    let terminal = sink
        .terminal
        .lock()
        .expect("live smoke terminal lock should not be poisoned")
        .clone()
        .expect("live stream must emit exactly one terminal result")
        .expect("live stream terminal must be successful");
    let first_pcm_elapsed = sink
        .first_pcm_at
        .lock()
        .expect("live smoke first-PCM lock should not be poisoned")
        .expect("live stream must record a first PCM timestamp")
        .duration_since(task_started);

    assert!(
        terminal.chunk_count() > 0,
        "live 终态必须统计至少一个 PCM chunk"
    );
    assert!(terminal.byte_count() > 0, "live 终态必须统计非零 PCM bytes");
    eprintln!(
        "MiniMax live baseline: catalog_ms={}, task_start_ms={}, first_pcm_ms={}, total_ms={}, chunks={}, bytes={}, usage_chars={}, trace_present={}, session_present={}",
        catalog_elapsed.as_millis(),
        task_start_elapsed.as_millis(),
        first_pcm_elapsed.as_millis(),
        total_started.elapsed().as_millis(),
        terminal.chunk_count(),
        terminal.byte_count(),
        terminal.usage_characters(),
        terminal.trace_id().is_some(),
        terminal.session_id().is_some(),
    );
}
