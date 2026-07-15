use super::*;

/// TTS-MANAGER-1：取消中的模型不能继续被展示或使用为 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 2（精选 Matcha 与 Kokoro）
///   download total  = 100 bytes（便于观察 40% 的已下载进度）
///   target entry    = 1（第一个 Matcha 条目）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从内置目录构造未扫描状态，读取第一与第二个条目；
///   2. 把第一条推进到 Installing(40 / 100)；
///   3. 请求取消第一条下载，再模拟一个迟到的 90 / 100 进度事件；
///   4. 重新读取两个条目的状态。
///
/// 预期结果：
///   - 断言 A：第一条必须保持 Cancelling，迟到进度不能将其复活为 Installing 或 Ready；
///   - 断言 B：第二条必须保持 NotInstalled，取消一项不能污染另一项。
#[test]
fn test_cancelling_one_tts_install_never_marks_any_entry_ready() {
    let catalog = tts::builtin_model_catalog();
    let first_id = catalog.entries()[0].id().clone();
    let second_id = catalog.entries()[1].id().clone();
    let mut state = TtsModelManagerState::unscanned(&catalog);

    state.begin_install(&first_id, 100);
    state.update_progress(&first_id, 40, 100);
    state.begin_cancellation(&first_id);
    state.update_progress(&first_id, 90, 100);

    assert!(
        matches!(
            state.installation(&first_id),
            Some(TtsModelInstallState::Cancelling {
                downloaded: 40,
                total: 100
            })
        ),
        "取消后的迟到进度不得覆盖 40/100 的 Cancelling 状态或把下载复活"
    );
    assert!(
        matches!(
            state.installation(&second_id),
            Some(TtsModelInstallState::NotInstalled)
        ),
        "取消第一条下载不得把未操作的第二条标记为 Installing、Cancelling 或 Ready"
    );
}

/// TTS-DOWNLOAD-1：响应体停滞时，下载必须在读取超时内失败并清理临时文件。
///
/// 数据构造（含关键数值的推导过程）：
///   response headers delay = 0ms（先让客户端下载到响应头并创建 partial 文件）
///   response body delay    = 500ms（模拟服务端卡住）
///   receive timeout        = 200ms（测试专用，低于 500ms 停滞时长）
///   required return bound  = 350ms（小于 500ms，证明不是等待服务端主动断开）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 本地 TcpListener 先返回 Content-Length: 1，随后 500ms 不发送响应体；
///   2. download_file_with_body_timeout 在读取响应体时等待网络数据；
///   3. 下载层应在超时后返回错误，并删除已创建的 `.partial` 文件；
///   4. 假 server 在其延迟结束后关闭连接，测试线程随后回收它。
///
/// 预期结果：
///   - 断言 A：调用在 350ms 内失败，不能等待 500ms 的服务端断连；
///   - 断言 B：稳定归档不存在，超时结果不能被安装器消费；
///   - 断言 C：`.partial` 不存在，重试不继承卡住下载的残留状态。
#[test]
fn test_tts_download_timeout_removes_partial_before_stalled_server_closes() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").expect("应绑定停滞响应假 server");
    let address = listener.local_addr().expect("应读取停滞响应 server 地址");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("停滞响应 server 应接受连接");
        let mut request = [0_u8; 1024];
        let _ = stream
            .read(&mut request)
            .expect("停滞响应 server 应读取请求");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n")
            .expect("停滞响应 server 应先写入响应头");
        stream.flush().expect("停滞响应 server 应立即发送响应头");
        std::thread::sleep(Duration::from_millis(500));
    });
    let temp = tempfile::tempdir().expect("应创建隔离的 TTS 下载目录");
    let archive = temp.path().join("stalled-model.tar.bz2");
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();

    let result = download_file_with_body_timeout(
        &format!("http://{address}/stalled-model.tar.bz2"),
        1,
        archive.clone(),
        &cancelled,
        |_downloaded| {},
        Duration::from_millis(200),
    );
    let elapsed = started.elapsed();
    server.join().expect("停滞响应 server 线程应正常结束");

    assert!(result.is_err(), "响应体在超时内没有数据时下载必须返回错误");
    assert!(
        elapsed < Duration::from_millis(350),
        "下载应在 350ms 内超时返回，实际耗时为 {elapsed:?}，不应等待服务端 500ms 后断连"
    );
    assert!(
        !archive.exists(),
        "超时下载不得产生可被安装器消费的最终归档"
    );
    assert!(
        !archive.with_extension("partial").exists(),
        "超时下载必须清理 partial 文件，避免重试继承残留状态"
    );
}
