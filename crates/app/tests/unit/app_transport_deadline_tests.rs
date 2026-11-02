use super::*;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// 场景：不足每次read超时的无换行碎片，不能无限延长同一条NDJSON请求。
/// 构造：真实Socket pair，50×75ms=3.75秒慢滴，每次间隔远小于原2秒流超时。
/// 执行：实际handle_stream读取，返回后回收发送方与线程。
/// 预期：整条请求在3秒内失败、不进入App队列；每次读到碎片都重新计时会使此测试失败。
#[test]
fn dripping_partial_line_has_one_request_deadline() {
    let (mut client, bridge) = UnixStream::pair().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..50 {
            if stopped.load(Ordering::Acquire) || client.write_all(b" ").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(75));
        }
    });
    let (sender, requests) = bounded(1);
    let start = Instant::now();
    handle_stream(bridge, sender, &AtomicBool::new(false));
    let elapsed = start.elapsed();
    stop.store(true, Ordering::Release);
    writer.join().unwrap();
    assert!(
        elapsed < Duration::from_secs(3),
        "碎片不能续期整条请求：{elapsed:?}"
    );
    assert!(requests.try_recv().is_err(), "未完成NDJSON不能进入App队列");
}

/// 场景：读取预算按UTF8字节计数，字符跨read到达也不能被误判为损坏。
/// 构造：é占2字节，加换行共3字节，恰好等于预算；第一个UTF8字节单独送达。
/// 执行：用真实Socket分段写入，并在换行后附加另一条数据。
/// 预期：只返回完整首行，不读入换行后的数据，也不把未完整的字符提前解码。
#[test]
fn request_limit_counts_complete_utf8_wire_bytes() {
    let (mut client, mut bridge) = UnixStream::pair().unwrap();
    let writer = std::thread::spawn(move || {
        client.write_all(&[0xc3]).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        client.write_all(&[0xa9, b'\n', b'x']).unwrap();
    });
    let line = read_request_line(&mut bridge, &AtomicBool::new(false), 3).unwrap();
    writer.join().unwrap();
    assert_eq!(line, "é\n", "UTF8跨read必须保留且只读一条帧");
    assert_eq!(line.len(), 3, "预算必须按字节而非字符计算");
}

/// 场景：输入达到资源边界后还多1字节必须拒绝。
/// 构造：4字节预算与5字节完整行abcd换行，不需要分配生产64MiB测试数据。
/// 执行：通过实际有界读取函数处理真实Socket数据。
/// 预期：InvalidData，而不是截断为合法请求或等待更多正文。
#[test]
fn oversized_request_is_rejected_without_truncation() {
    let (mut client, mut bridge) = UnixStream::pair().unwrap();
    client.write_all(b"abcd\n").unwrap();
    let error = read_request_line(&mut bridge, &AtomicBool::new(false), 4).unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::InvalidData,
        "超限不能截断或改写请求"
    );
}
