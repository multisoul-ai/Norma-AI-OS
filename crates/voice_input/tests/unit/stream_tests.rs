use super::{forward_pcm_events, PcmStreamSession};
use crate::PcmEvent;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// STREAM-1: drop PcmStreamSession 应停止 capture thread 并关闭 chunk stream。
///
/// 数据构造（含关键数值的推导过程）：
///   pcm queue capacity = 1（测试不发送样本，只验证关闭路径）
///   chunk queue capacity = 1（测试不发送 chunk，只观察 sender drop 后 Receiver::Closed）
///   stop_count expected = 1（drop 只应发送一次停止信号）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个 fake capture thread，收到 stop_rx 后标记 stopped=true 并 drop pcm_tx
///   2. 构造 forward thread，运行生产 forward_pcm_events(pcm_rx, chunk_tx)
///   3. 将两个 thread handle 放入 PcmStreamSession
///   4. drop session，触发 stop signal、join capture、join forward
///
/// 预期结果：
///   - 正断言：stopped=true，说明 drop 停止了 capture 所有权路径
///   - 正断言：chunk_rx 变为 Closed，说明 stream sender 随 forward thread 退出而关闭
///   - 负断言：stop_count 不应为 0，避免 drop 泄漏后台 capture
#[test]
fn test_drop_pcm_stream_session_stops_capture_and_closes_stream() {
    let stopped = Arc::new(AtomicBool::new(false));
    let stop_count = Arc::new(AtomicUsize::new(0));
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (pcm_tx, pcm_rx) = crossbeam_channel::bounded::<PcmEvent>(1);
    let (chunk_tx, chunk_rx) = async_channel::bounded(1);

    let stopped_for_thread = Arc::clone(&stopped);
    let stop_count_for_thread = Arc::clone(&stop_count);
    let capture_thread = std::thread::spawn(move || {
        let _ = stop_rx.recv();
        stop_count_for_thread.fetch_add(1, Ordering::SeqCst);
        stopped_for_thread.store(true, Ordering::SeqCst);
        drop(pcm_tx);
    });
    let forward_thread = std::thread::spawn(move || forward_pcm_events(pcm_rx, chunk_tx, None));

    let session = PcmStreamSession {
        stop_tx: Some(stop_tx),
        capture_thread: Some(capture_thread),
        forward_thread: Some(forward_thread),
        #[cfg(feature = "test-support")]
        test_hold_tx: None,
    };
    drop(session);

    assert!(
        stopped.load(Ordering::SeqCst),
        "dropping PcmStreamSession should notify and join the capture owner thread"
    );
    assert_eq!(
        stop_count.load(Ordering::SeqCst),
        1,
        "drop should send exactly one stop notification to the capture owner"
    );
    assert!(
        matches!(
            chunk_rx.try_recv(),
            Err(async_channel::TryRecvError::Closed)
        ),
        "dropping PcmStreamSession should close the PCM chunk receiver once forwarder exits"
    );
}

/// STREAM-2: cancel(self) 应通过所有权消费保证重复取消安全。
///
/// 数据构造（含关键数值的推导过程）：
///   stop_count expected = 1（一次 cancel 只停止一次 capture）
///   cancel receiver capacity = std::sync::mpsc 默认无界（只发送一个 stop signal）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只含 fake capture thread 的 PcmStreamSession
///   2. 调用 session.cancel()，该方法按签名消费 self
///   3. 读取 stream.rs 源码，确认公开签名是 cancel(self) 而不是 cancel(&mut self)
///
/// 预期结果：
///   - 正断言：cancel 后 capture thread 收到 1 次 stop
///   - 正断言：源码签名消费 self，调用者无法对同一个 session 再调用第二次 cancel
///   - 负断言：源码不应暴露 &mut self cancel，避免运行时重复取消状态
#[test]
fn test_cancel_consumes_session_and_stops_capture_once() {
    let stop_count = Arc::new(AtomicUsize::new(0));
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let stop_count_for_thread = Arc::clone(&stop_count);
    let capture_thread = std::thread::spawn(move || {
        let _ = stop_rx.recv();
        stop_count_for_thread.fetch_add(1, Ordering::SeqCst);
    });

    let session = PcmStreamSession {
        stop_tx: Some(stop_tx),
        capture_thread: Some(capture_thread),
        forward_thread: None,
        #[cfg(feature = "test-support")]
        test_hold_tx: None,
    };
    session.cancel();

    assert_eq!(
        stop_count.load(Ordering::SeqCst),
        1,
        "cancel(self) should stop the capture owner exactly once"
    );

    let source = include_str!("../../src/stream.rs");
    assert!(
        source.contains("pub fn cancel(mut self)"),
        "PcmStreamSession::cancel should consume self so a second call on the same value cannot compile"
    );
    assert!(
        !source.contains("pub fn cancel(&mut self)"),
        "PcmStreamSession::cancel must not use &mut self because that would allow repeated runtime calls"
    );
}

/// STREAM-3: start_pcm_stream 仍必须先检查麦克风权限，且不得创建 sherpa recognizer。
///
/// 数据构造（含关键数值的推导过程）：
///   auth call = ensure_microphone_authorized()?（统一 macOS 麦克风授权入口）
///   stream path = spawn_capture_thread / AudioCapture::start（开始访问真实输入设备）
///   recognizer symbols = build_recognizer_config / OnlineRecognizer（one-shot 专用）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 stream.rs 源码
///   2. 定位 ensure_microphone_authorized()? 与 capture 启动位置
///   3. 确认授权检查在 capture 启动之前
///   4. 确认 stream.rs 不包含 sherpa recognizer 构造符号
///
/// 预期结果：
///   - 正断言：start_pcm_stream 的 stream path 之前有麦克风权限检查
///   - 负断言：stream path 不应创建 recognizer 或解析 sherpa model
///   - 负断言：权限检查不应被 app 层假设替代
#[test]
fn test_start_pcm_stream_requires_microphone_authorization_without_recognizer() {
    let source = include_str!("../../src/stream.rs");
    let auth_pos = source
        .find("ensure_microphone_authorized()?")
        .expect("stream start should call ensure_microphone_authorized()? before capture starts");
    let capture_pos = source
        .find("spawn_capture_thread")
        .or_else(|| source.find("AudioCapture::start"))
        .expect("stream start should have a capture startup path");

    assert!(
        auth_pos < capture_pos,
        "start_pcm_stream should check microphone authorization before starting AVAudioEngine capture"
    );
    assert!(
        !source.contains("build_recognizer_config"),
        "PCM stream path should not build sherpa recognizer config"
    );
    assert!(
        !source.contains("OnlineRecognizer"),
        "PCM stream path should not create a sherpa OnlineRecognizer"
    );
}

/// STREAM-4：双工网络发送暂慢时，麦克风队列应保留最新音频而非旧音频。
///
/// 数据构造（含关键数值的推导过程）：输出队列容量 2，连续输入 3 个 PCM chunk；
///   每个 chunk 为 320 采样 × 2 字节 = 640 字节 ≈ 20 毫秒；
///   饱和后应保留第 2、3 个 chunk，丢弃第 1 个旧 chunk。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 网络消费者暂不取音频，依次送入 0.1、0.2、0.3 的采样。
///   2. 前向线程把前两段放入容量为 2 的队列。
///   3. 第三段到达时移除最旧段，再放入最新段。
///
/// 预期结果：队列中依次为 0.2 与 0.3；0.1 不存在，也没有额外旧音频。
#[test]
fn test_duplex_pcm_queue_discards_stale_audio_when_network_lags() {
    let (pcm_tx, pcm_rx) = crossbeam_channel::unbounded();
    let (chunk_tx, chunk_rx) = async_channel::bounded(2);
    for level in [0.1_f32, 0.2, 0.3] {
        pcm_tx
            .send(PcmEvent::Samples(vec![level; 320]))
            .expect("三个 20 毫秒 PCM 事件应进入测试输入队列");
    }
    drop(pcm_tx);
    forward_pcm_events(pcm_rx, chunk_tx, Some(chunk_rx.clone()));

    let first = chunk_rx.try_recv().expect("队列应保留第二段音频");
    let second = chunk_rx.try_recv().expect("队列应保留第三段音频");
    let first_sample = i16::from_le_bytes([first.bytes[0], first.bytes[1]]);
    let second_sample = i16::from_le_bytes([second.bytes[0], second.bytes[1]]);
    // 0.2 × 32767 ≈ 6553；0.3 × 32767 ≈ 9830；旧 0.1 × 32767 ≈ 3277。
    assert_eq!(first_sample, 6553, "第一段应是第二个 20 毫秒音频块");
    assert_eq!(second_sample, 9830, "第二段应是最新的 20 毫秒音频块");
    assert_ne!(first_sample, 3277, "网络恢复后不得先发送积压的旧音频");
    assert!(
        matches!(
            chunk_rx.try_recv(),
            Err(async_channel::TryRecvError::Closed)
        ),
        "前向结束后不得残留第三段积压音频"
    );
}

/// STREAM-4：完整 PcmStreamSession 启动后必须把 AVAudioEngine tap 数据转发到异步队列。
///
/// 数据构造（含关键数值的推导过程）：
///   input device       = None（使用系统默认麦克风，与当前 App 配置一致）
///   poll interval      = 10ms
///   max polls          = 200
///   total timeout      = 10ms × 200 = 2,000ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用生产 `start(None)`，创建 capture thread、AVAudioEngine 与 forward thread。
///   2. 每 10ms 从异步 PCM receiver 读取一次，最多等待 200 次。
///   3. 收到首块 PCM 后取消 session，确保 capture/forward thread 正常回收。
///
/// 预期结果：
///   - 正断言：2 秒内至少收到一块非空 PCM，证明完整实时链路持续产出。
///   - 负断言：不得只报告 AVAudioEngine started，却始终没有 PCM chunk。
#[test]
#[ignore = "requires microphone hardware and permission"]
fn test_pcm_stream_session_forwards_microphone_chunks() {
    let (session, pcm_rx) =
        super::start_pcm_stream(None).expect("完整 PCM stream 应能使用已授权的系统默认麦克风启动");
    let mut received = None;

    for _ in 0..200 {
        match pcm_rx.try_recv() {
            Ok(chunk) => {
                received = Some(chunk);
                break;
            }
            Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }
    session.cancel();

    let chunk = received.expect("AVAudioEngine started 后 2 秒内应有 PCM 到达异步转发队列");
    assert!(
        !chunk.bytes.is_empty(),
        "完整 PCM stream 收到的首块音频不得为空"
    );
}

/// STREAM-5：豆包关闭后重开，两个会话都必须向 ASR 提供真实 PCM。
///
/// 数据构造（含关键数值的推导过程）：16 kHz 单声道 PCM16 为 32,000 字节/秒；
/// 每次轮询 10 ms，最多 200 次 = 每个会话 2 秒；两次分别至少收到一段非空音频。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用系统默认麦克风启动豆包专用流，接收首段 PCM 后取消并等待线程退出。
///   2. 再次启动同一入口，接收新会话首段 PCM 后取消并等待线程退出。
///
/// 预期结果：两次启动均有真实 16 kHz PCM；不能仅显示已启动而没有音频；
/// 两次均须保留回声消除；双工队列容量固定为 8 个 chunk，不允许裸录回退。
#[test]
#[ignore = "requires microphone hardware and permission"]
fn test_duplex_pcm_stream_restarts_with_live_audio() {
    fn capture_once() -> (Option<audio::PcmAudioChunk>, Option<usize>) {
        let (session, pcm_rx) = super::start_duplex_pcm_stream(None)
            .expect("豆包专用 PCM stream 应能用系统默认麦克风启动");
        let queue_capacity = pcm_rx.capacity();
        let mut received = None;
        for _ in 0..200 {
            match pcm_rx.try_recv() {
                Ok(chunk) => {
                    received = Some(chunk);
                    break;
                }
                Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(async_channel::TryRecvError::Closed) => break,
            }
        }
        session.cancel();
        (received, queue_capacity)
    }
    let (first, first_capacity) = capture_once();
    let (second, second_capacity) = capture_once();
    let first = first.expect("首次豆包会话 2 秒内必须有 PCM");
    let second = second.expect("重开豆包会话 2 秒内必须有 PCM");
    assert_eq!(
        first_capacity,
        Some(8),
        "首次会话的双工队列只能缓存 8 个 chunk"
    );
    assert_eq!(
        second_capacity,
        Some(8),
        "重开后的双工队列仍只能缓存 8 个 chunk"
    );
    assert!(!first.bytes.is_empty(), "首次会话首段 PCM 不能是空字节");
    assert!(!second.bytes.is_empty(), "重开会话首段 PCM 不能是空字节");
    assert_eq!(
        first.sample_rate_hz, 16_000,
        "首次豆包上行应使用 16 kHz PCM"
    );
    assert_eq!(
        second.sample_rate_hz, 16_000,
        "重开豆包上行应使用 16 kHz PCM"
    );
    assert_ne!(
        second.sample_rate_hz, 24_000,
        "豆包上行不能误用 24 kHz 播放采样率"
    );
}

/// STREAM-6：关闭后重建回声消除引擎，两次采集都必须产生经过 Voice Processing 的 PCM。
///
/// 数据构造（含关键数值的推导过程）：
///   两次采集都传入 voice_processing=true；不允许以原始麦克风回退冒充成功。
///   每次最多等待 800 ms，与生产 DUPLEX_CAPTURE_READY_TIMEOUT 相同。
///   队列容量为 1：只检查首块 PCM，后续音频可丢弃，不积压录音。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用生产 capture owner 线程启动 Voice Processing，等待首块 PCM。
///   2. 发送 stop 并 join，确保旧引擎已退出，再以同样设置启动第二次。
///   3. 分别检查首次与重开结果，防止普通采集掩盖 AEC 失效。
///
/// 预期结果：首次与重开均收到非空 Samples；不能只报告启动成功而返回超时。
#[test]
#[ignore = "requires microphone hardware and permission"]
fn test_voice_processing_capture_restarts_without_raw_fallback() {
    fn capture_once() -> Result<PcmEvent, crossbeam_channel::RecvTimeoutError> {
        let (pcm_tx, pcm_rx) = crossbeam_channel::bounded(1);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let owner = super::spawn_capture_thread(
            pcm_tx,
            stop_rx,
            started_tx,
            None,
            true,
            Arc::new(crate::AudioQueueStats::default()),
        )
        .expect("回声消除采集线程应启动");
        let startup = started_rx.recv().expect("采集线程应报告启动结果");
        let pcm = pcm_rx.recv_timeout(super::DUPLEX_CAPTURE_READY_TIMEOUT);
        let _ = stop_tx.send(());
        owner.join().expect("旧引擎必须完整关闭后才能重开");
        startup.expect("Voice Processing 引擎应成功启动");
        pcm
    }

    crate::ensure_microphone_authorized().expect("真实麦克风回归测试需要已授权设备");
    let first = capture_once();
    let reopened = capture_once();
    assert!(
        matches!(first, Ok(PcmEvent::Samples(ref samples)) if !samples.is_empty()),
        "首次回声消除采集必须产出 PCM，实际为 {first:?}"
    );
    assert!(
        matches!(reopened, Ok(PcmEvent::Samples(ref samples)) if !samples.is_empty()),
        "关闭后重开的回声消除采集必须产出 PCM，实际为 {reopened:?}"
    );
    assert!(
        !matches!(reopened, Err(crossbeam_channel::RecvTimeoutError::Timeout)),
        "不能以禁用回声消除的原始采集掩盖重开超时"
    );
}

/// STREAM-7：没有收到 AEC 音频时必须返回启动错误，不能把会话标记为可用。
///
/// 数据构造（含关键数值的推导过程）：sent_chunks 初始为 0；timeout=0 表示已到截止时间，
///   测试无需真实等待；记录一次成功回调后 sent_chunks=0+1=1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对零回调的采集执行生产就绪检查，模拟重开后引擎不产出 PCM。
///   2. 记录一块经过 Voice Processing 的 PCM，再次检查就绪状态。
///
/// 预期结果：零回调必须报错且不能返回成功；有真实回调后才能继续会话。
#[test]
fn test_voice_processing_timeout_rejects_unprotected_capture() {
    let stats = crate::AudioQueueStats::default();
    let result = super::wait_for_voice_processing_ready(&stats, std::time::Duration::ZERO);
    assert_eq!(
        result,
        Err(crate::VoiceInputError::Audio(
            "回声消除未能启动，请重新开启语音对话".into()
        )),
        "AEC 无音频时应返回明确错误，由调用方终止会话"
    );
    assert!(result.is_err(), "没有 AEC 音频不能报告采集就绪");

    stats.record_sent_chunk();
    assert_eq!(
        super::wait_for_voice_processing_ready(&stats, std::time::Duration::ZERO),
        Ok(()),
        "已收到 AEC 音频的会话应立即通过检查"
    );
}
