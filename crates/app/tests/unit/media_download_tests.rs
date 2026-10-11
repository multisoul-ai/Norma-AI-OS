use super::{download_from_url, DownloadCancellation};
use media_store::{MediaArtifact, MediaStore};
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

// 场景：依次收到截断正文、同长度损坏正文、HTTP 错误，再收到原始 abc。
// 每次连接使用同一可信长度与 SHA，失败必须清理 partial；成功后原字节可离线复用。
#[test]
fn integrity_failures_never_publish_and_success_reuses_verified_original_offline() {
    let fixture = tempfile::tempdir().expect("isolated cache fixture must exist");
    let store = MediaStore::new(fixture.path().to_path_buf());
    let artifact = MediaArtifact::new(
        "wallpapers/test.mp4",
        "https://example.com/original.mp4",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .expect("abc metadata must be valid");
    let responses: [&[u8]; 4] = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nab",
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabd",
        b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
    ];
    for (index, response) in responses.into_iter().enumerate() {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("local HTTP fixture must bind");
        let address = listener.local_addr().expect("fixture address must exist");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("download must connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("fixture request must have a timeout");
            let mut request = [0; 4096];
            let received = stream.read(&mut request).expect("request must arrive");
            assert!(
                received > 0,
                "fixture must receive request bytes before replying"
            );
            stream
                .write_all(response)
                .expect("fixture must send its body");
        });
        let (events, _) = async_channel::bounded(8);
        let outcome = download_from_url(
            &artifact,
            &store,
            &DownloadCancellation::default(),
            &events,
            &format!("http://{address}/original.mp4"),
        );
        server.join().expect("fixture server must stop");
        if index < 3 {
            assert!(
                outcome.is_err(),
                "invalid response {index} must fail installation"
            );
            assert_eq!(
                std::fs::read_dir(fixture.path())
                    .expect("cache must be readable")
                    .count(),
                0,
                "invalid response {index} must leave no installed or partial media"
            );
        } else {
            let path = outcome.expect("valid original must install successfully");
            assert_eq!(
                std::fs::read(&path).expect("installed original must be readable"),
                b"abc",
                "successful download must preserve every original byte"
            );
        }
    }
    let (events, _) = async_channel::bounded(8);
    let cached = download_from_url(
        &artifact,
        &store,
        &DownloadCancellation::default(),
        &events,
        "http://127.0.0.1:1/offline",
    )
    .expect("verified media must work with server offline");
    std::fs::write(&cached, b"abd").expect("fixture must simulate an externally corrupted cache");
    assert!(
        download_from_url(
            &artifact,
            &store,
            &DownloadCancellation::default(),
            &events,
            "http://127.0.0.1:1/offline"
        )
        .is_err(),
        "same-length corrupted cache must never be reused offline"
    );
}

// 场景：服务器已返回 HTTP 头但卡在正文，用户取消下载。
// 构造 loopback 服务声明 3 字节，仅发送首字节 a，可信原始内容为 abc。
// 等受管临时文件已写入首字节后取消，证明已进入正文；预期 2 秒内结束且清理。
#[test]
fn cancellation_interrupts_a_stalled_response_body() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("local fixture must bind");
    let address = listener
        .local_addr()
        .expect("fixture address must be known");
    let (headers_sent, waiting) = std::sync::mpsc::channel();
    let (stop, shutdown) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("test request must connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("fixture cannot block forever");
        let mut request = [0; 4096];
        let received = stream.read(&mut request).expect("HTTP request must arrive");
        assert!(
            received > 0,
            "fixture must receive HTTP request bytes before replying"
        );
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\na")
            .expect("headers and first byte must send");
        headers_sent.send(()).expect("worker must signal headers");
        let _ = shutdown.recv_timeout(Duration::from_secs(5));
    });
    let fixture = tempfile::tempdir().expect("isolated cache fixture must exist");
    let store = MediaStore::new(fixture.path().to_path_buf());
    let artifact = MediaArtifact::new(
        "wallpapers/test.mp4",
        "https://example.com/original.mp4",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .expect("abc metadata must be valid");
    let cancellation = Arc::new(DownloadCancellation::default());
    let worker_cancel = cancellation.clone();
    let (events, _) = async_channel::bounded(8);
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let value = download_from_url(
            &artifact,
            &store,
            &worker_cancel,
            &events,
            &format!("http://{address}/original.mp4"),
        );
        finished
            .send(value)
            .expect("test must receive download result");
    });
    waiting
        .recv_timeout(Duration::from_secs(5))
        .expect("server must have sent its headers");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut body_started = false;
    while std::time::Instant::now() < deadline {
        body_started = std::fs::read_dir(fixture.path())
            .expect("cache fixture must be readable")
            .flatten()
            .any(|file| file.metadata().is_ok_and(|metadata| metadata.len() == 1));
        if body_started {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cancellation.cancel();
    let outcome = result.recv_timeout(Duration::from_secs(2));
    let _ = stop.send(());
    server.join().expect("fixture server must stop");
    worker.join().expect("cancelled downloader must stop");
    assert!(
        body_started,
        "test must prove the original-media installer entered the stalled body"
    );
    assert!(
        outcome
            .expect("cancellation must interrupt a stalled body within two seconds")
            .is_err(),
        "cancelled response must not publish successful original media"
    );
    assert_eq!(
        std::fs::read_dir(fixture.path())
            .expect("cache must be readable")
            .count(),
        0,
        "cancelled stalled body must clean every partial file"
    );
}

// 场景：同时请求多个不同媒体，两下载占满限额后第三任务排队并被取消。
// 预期第三请求不能立即获得许可，取消无需等前两任务完成，随后许可可复用。
#[tokio::test]
async fn queued_download_is_bounded_and_can_be_cancelled_before_network_starts() {
    let first_cancel = DownloadCancellation::default();
    let second_cancel = DownloadCancellation::default();
    let queued_cancel = DownloadCancellation::default();
    let first = super::acquire_slot(&first_cancel)
        .await
        .expect("first download must receive a slot");
    let second = super::acquire_slot(&second_cancel)
        .await
        .expect("second download must receive a slot");
    let queued = super::acquire_slot(&queued_cancel);
    tokio::pin!(queued);
    tokio::select! {
        _ = &mut queued => panic!("third unique download must remain queued while both slots are held"),
        _ = tokio::time::sleep(Duration::from_millis(20)) => {},
    }
    queued_cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), &mut queued)
            .await
            .expect("cancelled queued download must stop promptly")
            .is_err(),
        "queued cancellation must never start network IO"
    );
    drop((first, second));
    assert!(
        super::acquire_slot(&DownloadCancellation::default())
            .await
            .is_ok(),
        "completed slots must remain reusable after queued cancellation"
    );
}
