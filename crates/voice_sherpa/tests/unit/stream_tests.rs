use super::{forward_pcm_events, PcmStreamSession};
use crate::session::PcmEvent;
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
    let forward_thread = std::thread::spawn(move || forward_pcm_events(pcm_rx, chunk_tx));

    let session = PcmStreamSession {
        stop_tx: Some(stop_tx),
        capture_thread: Some(capture_thread),
        forward_thread: Some(forward_thread),
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
        "dropping PcmStreamSession should close the realtime chunk receiver once forwarder exits"
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
