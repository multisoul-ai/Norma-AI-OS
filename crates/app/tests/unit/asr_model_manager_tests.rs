use super::*;
use asr::{builtin_model_catalog, InstallationManifest, ZIPFORMER_STREAMING_ZH_MODEL_ID};

/// ASR-MANAGER-1：后台扫描只能把清单与必要文件完整的模型标记为 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 2（Paraformer + Zipformer）
///   complete installs = 1（仅 Zipformer）
///   Zipformer required files = 4（encoder + decoder + joiner + tokens）
///   shared VAD files = 1（silero_vad/silero_vad.onnx）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先构造完整 Zipformer 安装与清单，但不放共享 VAD → 生产 scan 必须 Failed
///   2. 再放共享 VAD 占位文件，并通过注入的存在性校验模拟已哈希验证的支持工件
///   3. 分别读取 Zipformer 与 Paraformer 安装状态
///
/// 预期结果：
///   - 正断言：完整 Zipformer 为 Ready，未安装 Paraformer 为 NotInstalled
///   - 负断言：缺失共享 VAD 时不得 Ready；缺失目录的 Paraformer 也不得 Ready
#[test]
fn test_model_manager_scan_derives_ready_only_from_valid_installations() {
    let temp = tempfile::tempdir().expect("测试应创建模型根目录");
    let catalog = builtin_model_catalog();
    let zipformer_id =
        asr::AsrModelId::parse(ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("Zipformer ID 应有效");
    let zipformer = catalog.find(&zipformer_id).expect("模型库应包含 Zipformer");
    let zipformer_root = temp.path().join(zipformer_id.as_str());
    std::fs::create_dir_all(&zipformer_root).expect("应创建 Zipformer 安装目录");
    for file in zipformer.artifact().required_files() {
        std::fs::write(zipformer_root.join(file), [1_u8]).expect("应写入必要模型占位文件");
    }
    InstallationManifest::for_descriptor(zipformer)
        .write_to(&zipformer_root)
        .expect("应写入安装清单");
    let without_vad = AsrModelManagerState::scan(&catalog, temp.path());
    assert!(
        matches!(
            without_vad.installation(&zipformer_id),
            Some(AsrModelInstallState::Failed(message)) if message.contains("silero_vad")
        ),
        "ASR 完整但缺少共享 Silero VAD 时不得显示 Ready"
    );
    let vad_root = temp.path().join(asr::SILERO_VAD_DIRECTORY);
    std::fs::create_dir_all(&vad_root).expect("应创建共享 VAD 目录");
    std::fs::write(vad_root.join(asr::SILERO_VAD_FILE), [1_u8]).expect("应写入共享 VAD 占位文件");

    let state = AsrModelManagerState::scan_with_vad(&catalog, temp.path(), |root| {
        root.join(asr::SILERO_VAD_DIRECTORY)
            .join(asr::SILERO_VAD_FILE)
            .is_file()
            .then_some(())
            .ok_or_else(|| "shared VAD missing".to_string())
    });
    let paraformer_id =
        asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID).expect("Paraformer ID 应有效");

    assert_eq!(
        state.installation(&zipformer_id),
        Some(&AsrModelInstallState::Ready),
        "清单和四个必要文件完整的 Zipformer 应为 Ready"
    );
    assert_eq!(
        state.installation(&paraformer_id),
        Some(&AsrModelInstallState::NotInstalled),
        "没有受管目录的 Paraformer 应为 NotInstalled"
    );
    assert_ne!(
        state.installation(&paraformer_id),
        Some(&AsrModelInstallState::Ready),
        "缺失 Paraformer 文件时不得伪造 Ready 状态"
    );
}

/// ASR-MANAGER-3：存在但损坏的受管模型必须显示 Failed，而不是伪装成未安装。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 2
///   corrupt installs = 1（Zipformer 目录存在但 installation.json 缺失）
///   absent installs = 1（Paraformer 目录不存在）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 只创建 Zipformer 模型 ID 目录，不写清单
///   2. AsrModelManagerState::scan 分别检查存在目录与缺失目录
///   3. 读取两个模型的安装状态
///
/// 预期结果：
///   - 正断言：损坏 Zipformer 为带原因的 Failed，缺失 Paraformer 为 NotInstalled
///   - 负断言：损坏安装不得显示 NotInstalled 或 Ready
#[test]
fn test_model_manager_scan_distinguishes_corrupt_from_absent_installation() {
    let temp = tempfile::tempdir().expect("测试应创建模型根目录");
    let catalog = builtin_model_catalog();
    let zipformer_id =
        asr::AsrModelId::parse(ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("Zipformer ID 应有效");
    std::fs::create_dir_all(temp.path().join(zipformer_id.as_str()))
        .expect("应创建损坏 Zipformer 目录");

    let state = AsrModelManagerState::scan(&catalog, temp.path());
    let paraformer_id =
        asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID).expect("Paraformer ID 应有效");

    assert!(
        matches!(
            state.installation(&zipformer_id),
            Some(AsrModelInstallState::Failed(message)) if message.contains("installation")
        ),
        "存在但缺少清单的 Zipformer 应显示可重试 Failed 原因"
    );
    assert_eq!(
        state.installation(&paraformer_id),
        Some(&AsrModelInstallState::NotInstalled),
        "完全缺失的 Paraformer 才应显示 NotInstalled"
    );
    assert_ne!(
        state.installation(&zipformer_id),
        Some(&AsrModelInstallState::NotInstalled),
        "损坏 Zipformer 不得伪装成未安装"
    );
    assert_ne!(
        state.installation(&zipformer_id),
        Some(&AsrModelInstallState::Ready),
        "损坏 Zipformer 不得进入 Ready"
    );
}

/// ASR-MANAGER-2：安装进度、失败与重试必须是可恢复的显式状态迁移。
///
/// 数据构造（含关键数值的推导过程）：
///   total bytes      = 100
///   downloaded bytes = 40（进度 40 / 100）
///   retry bytes      = 0 / 100
///
/// 执行过程（逐步说明系统如何处理）：
///   1. begin_install → Installing 0/100
///   2. update_progress → Installing 40/100
///   3. mark_failed("network interrupted") → Failed
///   4. begin_install 重试 → Installing 0/100
///
/// 预期结果：
///   - 正断言：每个阶段可精确显示，重试会清除旧错误
///   - 负断言：失败状态不得被误判为 Ready
#[test]
fn test_model_manager_install_failure_can_retry_with_fresh_progress() {
    let catalog = builtin_model_catalog();
    let id = asr::AsrModelId::parse(ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("Zipformer ID 应有效");
    let mut state = AsrModelManagerState::unscanned(&catalog);

    state.begin_install(&id, 100);
    state.update_progress(&id, 40, 100);
    assert_eq!(
        state.installation(&id),
        Some(&AsrModelInstallState::Installing {
            downloaded: 40,
            total: 100,
        }),
        "下载进度应精确保留为 40/100"
    );

    state.mark_failed(&id, "network interrupted");
    assert_eq!(
        state.installation(&id),
        Some(&AsrModelInstallState::Failed(
            "network interrupted".to_string()
        )),
        "下载中断应进入带可读原因的 Failed 状态"
    );
    assert_ne!(
        state.installation(&id),
        Some(&AsrModelInstallState::Ready),
        "失败模型不得成为可选择的 Ready 模型"
    );

    state.begin_install(&id, 100);
    assert_eq!(
        state.installation(&id),
        Some(&AsrModelInstallState::Installing {
            downloaded: 0,
            total: 100,
        }),
        "重试应清除旧错误并从 0/100 开始"
    );
}

/// ASR-DOWNLOAD-1：应用下载层必须流式写入临时文件并报告确定进度。
///
/// 数据构造（含关键数值的推导过程）：
///   response bytes = 70,000（64 KiB buffer + 4,464 bytes，确保至少两次读取）
///   Content-Length = 70,000
///   expected final path suffix = test-download-model.tar.bz2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 本地 TcpListener 返回固定 Content-Length 与 70,000 字节响应体
///   2. download_model_archive 读取响应并回调累计进度
///   3. 下载完成后读取最终文件并检查 `.partial` 已消失
///
/// 预期结果：
///   - 正断言：最终字节、最后进度和稳定文件名精确匹配
///   - 负断言：成功后不得遗留 `.partial` 文件
#[test]
fn test_model_download_streams_local_http_response_and_finalizes_atomically() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("应绑定本地假 HTTP server");
    let address = listener.local_addr().expect("应读取假 server 地址");
    let body = vec![0x5a_u8; 70_000];
    let server_body = body.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("假 server 应接受一个连接");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).expect("假 server 应读取请求");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            server_body.len()
        )
        .expect("假 server 应写响应头");
        stream
            .write_all(&server_body)
            .expect("假 server 应写完整响应体");
    });
    let descriptor = asr::AsrModelDescriptor::new(
        asr::AsrModelId::parse("test-download-model").expect("测试模型 ID 应有效"),
        "Test Download Model",
        asr::AsrCapabilities::new(vec!["zh"], true),
        asr::AsrArtifact::new(
            format!("http://{address}/model.tar.bz2"),
            "unused-by-download-layer",
            70_000,
            vec!["tokens.txt"],
        ),
    );
    let temp = tempfile::tempdir().expect("应创建下载临时目录");
    let mut progress = Vec::new();

    let archive = download_model_archive(&descriptor, temp.path(), |downloaded, total| {
        progress.push((downloaded, total));
    })
    .expect("本地确定响应应下载成功");
    server.join().expect("假 server 线程应正常结束");
    let downloaded = std::fs::read(&archive).expect("应读取最终归档");

    assert_eq!(downloaded, body, "最终归档必须逐字节匹配 HTTP 响应体");
    assert_eq!(
        progress.last().copied(),
        Some((70_000, 70_000)),
        "最后一次进度必须精确达到 Content-Length"
    );
    assert_eq!(
        archive.file_name().and_then(|name| name.to_str()),
        Some("test-download-model.tar.bz2"),
        "完成归档应使用稳定模型 ID 文件名"
    );
    assert!(
        !temp
            .path()
            .join("test-download-model.tar.bz2.partial")
            .exists(),
        "成功下载后不得遗留 partial 文件"
    );
    assert!(
        progress.len() >= 2,
        "70,000 字节响应超过 64 KiB buffer，应产生至少两次进度更新"
    );
}

/// ASR-DOWNLOAD-2：HTTP 响应中断不得留下最终归档或 partial 文件。
///
/// 数据构造（含关键数值的推导过程）：
///   declared Content-Length = 10,000 bytes
///   actual response body    = 100 bytes
///   missing bytes           = 10,000 - 100 = 9,900 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 本地假 server 声明 10,000 字节但仅发送 100 字节后断开
///   2. download_model_archive 读取到 UnexpectedEof/传输错误
///   3. 检查稳定归档与 `.partial` 路径
///
/// 预期结果：
///   - 正断言：下载返回可重试错误
///   - 负断言：最终归档和 partial 文件都不存在
#[test]
fn test_model_download_interruption_removes_partial_and_never_finalizes() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("应绑定中断假 server");
    let address = listener.local_addr().expect("应读取中断假 server 地址");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("中断假 server 应接受连接");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).expect("中断假 server 应读取请求");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\nConnection: close\r\n\r\n")
            .expect("中断假 server 应写响应头");
        stream
            .write_all(&[0x2a_u8; 100])
            .expect("中断假 server 应写 100 字节后关闭");
    });
    let descriptor = asr::AsrModelDescriptor::new(
        asr::AsrModelId::parse("interrupted-download-model").expect("测试模型 ID 应有效"),
        "Interrupted Download Model",
        asr::AsrCapabilities::new(vec!["zh"], true),
        asr::AsrArtifact::new(
            format!("http://{address}/model.tar.bz2"),
            "unused-by-download-layer",
            10_000,
            vec!["tokens.txt"],
        ),
    );
    let temp = tempfile::tempdir().expect("应创建中断下载目录");

    let result = download_model_archive(&descriptor, temp.path(), |_downloaded, _total| {});
    server.join().expect("中断假 server 线程应正常结束");

    assert!(result.is_err(), "响应缺少 9,900 字节时下载必须返回错误");
    assert!(
        !temp
            .path()
            .join("interrupted-download-model.tar.bz2")
            .exists(),
        "中断下载不得产生可被安装器消费的最终归档"
    );
    assert!(
        !temp
            .path()
            .join("interrupted-download-model.tar.bz2.partial")
            .exists(),
        "中断下载应清理 partial 文件，允许后续干净重试"
    );
}
