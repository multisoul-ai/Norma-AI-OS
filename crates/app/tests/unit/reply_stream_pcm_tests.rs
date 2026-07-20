use super::*;
use audio::PcmFormat;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

// 20ms = 10 × 2ms planned cancellation polls: enough to prove the producer is waiting.
const BACKPRESSURE_OBSERVATION: Duration = Duration::from_millis(20);
// 1s = 500 × 2ms planned polls: a generous deterministic ceiling for local thread scheduling.
const TEST_DEADLINE: Duration = Duration::from_secs(1);

fn pcm_chunk() -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 32_000, 1, vec![0; 2_048])
        .expect("32ms MiniMax PCM fixture must be valid")
}

fn channel_sink(
    capacity: usize,
) -> (
    Arc<ChannelTtsChunkSink>,
    async_channel::Receiver<super::super::SpeechWorkerEvent>,
    Arc<AtomicBool>,
) {
    let (events, receiver) = async_channel::bounded(capacity);
    let cancelled = Arc::new(AtomicBool::new(false));
    (
        Arc::new(ChannelTtsChunkSink {
            events,
            cancelled: Arc::clone(&cancelled),
            tts_request_sequence: 1,
            next_chunk_sequence: AtomicU64::new(0),
            terminal: Mutex::new(None),
        }),
        receiver,
        cancelled,
    )
}

fn drain_until(receiver: &async_channel::Receiver<super::super::SpeechWorkerEvent>, target: usize) {
    let deadline = Instant::now() + TEST_DEADLINE;
    let mut received = 0;
    while received < target && Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(_) => received += 1,
            Err(async_channel::TryRecvError::Empty) => std::thread::yield_now(),
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }
    assert_eq!(
        received, target,
        "receiver 必须在 1 秒内收到目标数量的 PCM 事件"
    );
}

/// STR-BURST-1：MiniMax 的 47 块 PCM 突发必须通过有界背压完整交付。
///
/// 数据构造（含关键数值的推导过程）：
///   chunk bytes       = 2,048（真实日志中的 PCM 块大小）
///   chunk duration    = 2,048 / 2 bytes per sample / 32,000 Hz = 32ms
///   channel capacity  = 16 chunks = 16 × 32ms = 512ms
///   incoming chunks   = 47（日志最高 chunk_sequence=46，因此 0..46 共 47 块）
///   overflow portion  = 47 - 16 = 31 chunks
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先发送 16 块填满真实 bounded channel，receiver 暂不 drain。
///   2. 后台 producer 尝试发送剩余 31 块；第 17 块应等待而不是返回 false。
///   3. receiver 开始 drain，释放容量并接收全部 47 块。
///   4. 读取 producer 的 31 次 send 结果并等待线程退出。
///
/// 预期结果：
///   - 正断言：初始 16 块和后续 31 块全部成功，receiver 恰好收到 47 块。
///   - 负断言：producer 不得在 receiver drain 前结束，也不得出现 false/Cancelled。
#[test]
fn test_minimax_pcm_burst_backpressures_until_receiver_drains() {
    let (sink, receiver, _cancelled) = channel_sink(super::super::SPEECH_EVENT_CAPACITY);
    let initial_results = (0..16)
        .map(|_| tts::TtsChunkSink::send(sink.as_ref(), pcm_chunk()))
        .collect::<Vec<_>>();
    assert_eq!(receiver.len(), 16, "初始 16 块必须恰好填满 PCM 事件通道");
    assert!(
        initial_results.iter().all(|accepted| *accepted),
        "通道容量内的前 16 块必须全部交付成功"
    );

    let (completed_tx, completed_rx) = mpsc::sync_channel(1);
    let producer_sink = Arc::clone(&sink);
    let producer = std::thread::spawn(move || {
        let results = (16..47)
            .map(|_| tts::TtsChunkSink::send(producer_sink.as_ref(), pcm_chunk()))
            .collect::<Vec<_>>();
        completed_tx
            .send(results)
            .expect("test completion receiver must remain alive");
    });

    assert!(
        matches!(
            completed_rx.recv_timeout(BACKPRESSURE_OBSERVATION),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "通道满时 producer 必须等待 receiver drain，不得立即返回 false"
    );
    drain_until(&receiver, 47);
    let overflow_results = completed_rx
        .recv_timeout(TEST_DEADLINE)
        .expect("receiver drain 后 producer 必须在 1 秒内完成");
    producer
        .join()
        .expect("背压 producer 线程不得 panic 或滞留");

    assert_eq!(
        overflow_results.len(),
        31,
        "47 - 16 必须产生恰好 31 次背压后续发送"
    );
    assert!(
        overflow_results.iter().all(|accepted| *accepted),
        "receiver drain 后剩余 31 块必须全部成功，不得误报 Cancelled"
    );
    assert!(
        !overflow_results.iter().any(|accepted| !accepted),
        "未设置 cancellation flag 时任何 PCM 块都不得返回 false"
    );
}

/// STR-BURST-2：通道满时的后台发送必须响应真正的取消且不泄漏 PCM。
///
/// 数据构造（含关键数值的推导过程）：
///   channel capacity = 16 chunks（先填满）
///   waiting sends    = 1（第 17 块）
///   cancellation     = false → true（等待期间切换一次）
///   response ceiling = 1s = 500 × 2ms cancellation polls
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 填满 16-slot 通道并保持 receiver 不 drain。
///   2. 后台发送第 17 块，确认它进入背压等待。
///   3. 设置真实 cancellation flag，等待 producer 返回。
///   4. 检查返回值、通道长度和线程终态。
///
/// 预期结果：
///   - 正断言：取消后 producer 在 1 秒内退出，原 16 块仍保持有界。
///   - 负断言：第 17 块不得报告成功，也不得进入已满通道或造成线程死锁。
#[test]
fn test_backpressured_pcm_send_exits_when_stream_is_cancelled() {
    let (sink, receiver, cancelled) = channel_sink(super::super::SPEECH_EVENT_CAPACITY);
    let initial_results = (0..16)
        .map(|_| tts::TtsChunkSink::send(sink.as_ref(), pcm_chunk()))
        .collect::<Vec<_>>();
    assert!(
        initial_results.iter().all(|accepted| *accepted),
        "取消场景的前 16 块必须先完整填满通道"
    );

    let (completed_tx, completed_rx) = mpsc::sync_channel(1);
    let producer_sink = Arc::clone(&sink);
    let producer = std::thread::spawn(move || {
        completed_tx
            .send(tts::TtsChunkSink::send(producer_sink.as_ref(), pcm_chunk()))
            .expect("test cancellation receiver must remain alive");
    });
    assert!(
        matches!(
            completed_rx.recv_timeout(BACKPRESSURE_OBSERVATION),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "第 17 块必须先等待背压，不能在取消前提前结束"
    );

    cancelled.store(true, Ordering::Release);
    let accepted = completed_rx
        .recv_timeout(TEST_DEADLINE)
        .expect("设置取消后背压发送必须在 1 秒内退出");
    producer
        .join()
        .expect("取消后的 producer 线程不得 panic 或滞留");

    assert!(!accepted, "真正取消后第 17 块必须返回 false，不得继续交付");
    assert_eq!(
        receiver.len(),
        16,
        "取消后的第 17 块不得进入已满通道，容量必须仍为 16"
    );
    assert!(
        cancelled.load(Ordering::Acquire),
        "测试结束时 cancellation flag 必须保持 true，证明退出由真实取消触发"
    );
}
