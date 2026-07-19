use super::{AzureSpeechClient, AzureSpeechConfig, AzureSpeechRecognizer, AzureSpeechSecret};
use crate::{
    AsrProviderCapabilities, InputEndReason, RecognitionEvent, RecognitionPipeline, Recognizer,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::io::{Read, Write};
use std::net::TcpListener;

/// AZ-1：Azure 配置应由 region 推导 STT Endpoint，且 Debug 不泄露密钥。
///
/// 数据构造（含关键数值的推导过程）：
///   region = eastus
///   locale = zh-CN
///   secret = 可辨识 fake 值，仅用于检查脱敏
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 region、locale 和 secret 构造配置
///   2. 读取推导后的 Endpoint
///   3. 格式化配置 Debug 文本
///
/// 预期结果：
///   - Endpoint 应为 eastus 专用 STT 域名
///   - Debug 应包含 REDACTED
///   - Endpoint 不应是 cognitive 管理域名
///   - Debug 不应包含 fake secret
#[test]
fn azure_config_derives_public_stt_endpoint_and_redacts_secret() {
    let config = AzureSpeechConfig::new(
        "eastus",
        "zh-CN",
        AzureSpeechSecret::new("fake-sensitive-key"),
    )
    .expect("eastus 与 zh-CN 应构成有效 Azure 配置");

    assert_eq!(
        config.stt_endpoint(),
        "https://eastus.stt.speech.microsoft.com",
        "标准公共区域应推导专用 Speech STT Endpoint"
    );
    assert!(
        !config
            .stt_endpoint()
            .contains("api.cognitive.microsoft.com"),
        "识别请求不得误用 cognitive 管理 Endpoint"
    );
    let debug = format!("{config:?}");
    assert!(debug.contains("REDACTED"), "Debug 应明确表示密钥已脱敏");
    assert!(
        !debug.contains("fake-sensitive-key"),
        "Debug 不得包含 Azure API Key 明文"
    );
}

/// AZ-2：Azure Recognizer 应把标准 PCM 封装为 WAV 并返回服务端最终文本。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM = 20 ms × 16,000 samples/s × 2 bytes/sample = 640 bytes
///   WAV = 44 bytes header + 640 bytes PCM = 684 bytes
///   fake response = Success + “打开 Java 项目。”
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Azure 接收一次 HTTP POST 并返回成功 JSON
///   2. recognizer 接收 640 bytes PCM，不产生 partial
///   3. finish 把 PCM 封装为 WAV 并请求 fake Azure
///   4. 测试检查请求和返回文本
///
/// 预期结果：
///   - accept_pcm 不应产生 partial
///   - finish 应返回具体最终文本
///   - body 应具有 RIFF/WAVE 标识和 684 bytes 总长度
///   - 请求 URL 不应包含订阅密钥
#[test]
fn azure_recognizer_buffers_pcm_as_wav_and_returns_final_text() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Azure 应绑定临时端口");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("fake Azure 应暴露本地地址")
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fake Azure 应接受一次请求");
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 2048];
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .expect("fake Azure 应设置有界读取超时");
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    bytes.extend_from_slice(&buffer[..count]);
                    if request_is_complete(&bytes) {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => break,
                Err(error) => panic!("fake Azure 读取失败: {error}"),
            }
        }
        let body = r#"{"RecognitionStatus":"Success","DisplayText":"打开 Java 项目。"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("fake Azure 应写出完整响应");
        bytes
    });
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-unit-key"),
    );
    let mut recognizer = AzureSpeechRecognizer::new(AzureSpeechClient::new(config));
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640])
        .expect("640 bytes 应是合法 20ms PCM");

    let partials = recognizer
        .accept_pcm(&pcm)
        .expect("Azure recognizer 应接受标准 PCM");
    let final_text = recognizer.finish().expect("fake Azure Success 应完成识别");
    let request = server.join().expect("fake Azure server 应正常结束");

    assert!(partials.is_empty(), "Azure 短音频 REST 不应产生 partial");
    assert_eq!(
        final_text.as_deref(),
        Some("打开 Java 项目。"),
        "Success 响应应返回具体 DisplayText"
    );
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP 请求应包含 header/body 分隔符");
    let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
    let wav = &request[header_end + 4..];
    assert!(wav.starts_with(b"RIFF"), "Azure 请求 body 应以 RIFF 开头");
    assert!(
        wav.windows(4).any(|window| window == b"WAVE"),
        "Azure 请求 body 应包含 WAVE 标识"
    );
    assert_eq!(
        wav.len(),
        684,
        "20ms PCM 的 WAV 总长度应为 44 + 640 = 684 bytes"
    );
    assert!(
        !headers
            .lines()
            .next()
            .unwrap_or_default()
            .contains("fake-unit-key"),
        "Azure API Key 不得进入请求 URL"
    );
}

/// AZ-3：Azure 凭据验证只要求服务接受认证，不要求静音样本产生文本。
///
/// 数据构造（含关键数值的推导过程）：
///   validation PCM = 1,600 samples × 2 bytes = 3,200 bytes（100 ms 静音）
///   WAV body        = 44 + 3,200 = 3,244 bytes
///   fake status     = InitialSilenceTimeout（认证成功但没有语音）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Azure 接收验证请求并返回 HTTP 200 + InitialSilenceTimeout
///   2. client 运行 validate_credentials
///   3. 测试检查验证结果和固定短静音 WAV
///
/// 预期结果：
///   - 正断言：HTTP 层认证成功应判定配置可用
///   - 负断言：验证不应要求伪造 Final transcript
#[test]
fn azure_credential_validation_accepts_authenticated_silence_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Azure 应绑定临时端口");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("fake Azure 应暴露本地地址")
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fake Azure 应接受验证请求");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream.read(&mut buffer).expect("fake Azure 应读取请求");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if request_is_complete(&request) {
                break;
            }
        }
        let body = r#"{"RecognitionStatus":"InitialSilenceTimeout","DisplayText":""}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("fake Azure 应写出验证响应");
        request
    });
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-validation-key"),
    );
    let client = AzureSpeechClient::new(config);

    let result = client.validate_credentials();
    let request = server.join().expect("fake Azure 验证服务应正常结束");

    assert!(
        result.is_ok(),
        "认证成功的静音响应应使 Azure profile 进入 Ready"
    );
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("验证请求应包含 header/body 分隔符");
    assert_eq!(
        request.len() - header_end - 4,
        3_244,
        "100ms 静音验证请求应为 44 + 3,200 = 3,244 bytes"
    );
    assert!(
        !String::from_utf8_lossy(&request).contains("Final transcript"),
        "凭据验证不得伪造识别文本"
    );
}

/// AZ-4：Azure Success 但空 DisplayText 应映射为 Empty，而不是伪造文本。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM = 20 ms × 16,000 samples/s × 2 bytes/sample = 640 bytes
///   fake response = HTTP 200 + Success + 空 DisplayText
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Azure 返回成功但空文本
///   2. recognizer 缓冲 20ms PCM 并 finish
///
/// 预期结果：
///   - 正断言：finish 返回 None，供公共 pipeline 映射为 Empty
///   - 负断言：不得返回空字符串形式的伪 Final
#[test]
fn azure_success_with_empty_display_text_returns_none() {
    let (endpoint, server) = spawn_fake_response(
        "200 OK",
        r#"{"RecognitionStatus":"Success","DisplayText":""}"#,
    );
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-empty-key"),
    );
    let mut recognizer = AzureSpeechRecognizer::new(AzureSpeechClient::new(config));
    let pcm =
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640]).expect("20ms PCM 应构造成功");

    recognizer.accept_pcm(&pcm).expect("Azure 应接受 20ms PCM");
    let result = recognizer.finish().expect("Success 响应应完成解析");
    server.join().expect("fake Azure 应正常结束");

    assert!(result.is_none(), "空 DisplayText 应映射为 None/Empty");
    assert_ne!(
        result,
        Some(String::new()),
        "空文本不得伪装成可提交的 Final 字符串"
    );
}

/// AZ-5：Azure 非 Success 识别状态应成为明确 Provider 错误且保持密钥脱敏。
///
/// 数据构造（含关键数值的推导过程）：
///   WAV bytes = 44 bytes header（零 PCM）
///   fake response = HTTP 200 + NoMatch
///   secret marker = fake-nomatch-key
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 直接提交最小 WAV 到 fake Azure
///   2. client 解析 RecognitionStatus
///   3. 格式化返回错误
///
/// 预期结果：
///   - 正断言：错误包含 NoMatch，便于用户理解供应商结果
///   - 负断言：错误不得包含 API Key，也不得被当作空成功
#[test]
fn azure_non_success_status_is_explicit_and_secret_free() {
    let (endpoint, server) = spawn_fake_response(
        "200 OK",
        r#"{"RecognitionStatus":"NoMatch","DisplayText":""}"#,
    );
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-nomatch-key"),
    );
    let client = AzureSpeechClient::new(config);

    let result = client.transcribe_short_wav(b"RIFF");
    server.join().expect("fake Azure 应正常结束");
    let error = result.expect_err("NoMatch 不得被当作 Success").to_string();

    assert!(error.contains("NoMatch"), "错误应保留供应商 NoMatch 状态");
    assert!(
        !error.contains("fake-nomatch-key"),
        "供应商错误不得泄露 API Key"
    );
}

/// AZ-6：Azure HTTP 认证失败应映射为明确网络错误且不泄露密钥。
///
/// 数据构造（含关键数值的推导过程）：
///   HTTP status = 401 Unauthorized
///   request body = 4 bytes（仅用于触发 transport）
///   secret marker = fake-auth-key
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Azure 接收请求并返回 401
///   2. client 在 JSON 解码前检查 HTTP 状态
///   3. 测试读取映射后的错误文本
///
/// 预期结果：
///   - 正断言：错误包含 401
///   - 负断言：错误不得包含订阅密钥或成功文本
#[test]
fn azure_auth_failure_maps_http_status_without_secret() {
    let (endpoint, server) =
        spawn_fake_response("401 Unauthorized", r#"{"error":{"code":"Unauthorized"}}"#);
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-auth-key"),
    );
    let client = AzureSpeechClient::new(config);

    let error = client
        .transcribe_short_wav(b"RIFF")
        .expect_err("401 必须返回错误")
        .to_string();
    server.join().expect("fake Azure 应正常结束");

    assert!(error.contains("401"), "认证失败应保留 HTTP 401 状态");
    assert!(!error.contains("fake-auth-key"), "401 错误不得泄露 API Key");
    assert!(!error.contains("Success"), "401 不得被映射为识别成功");
}

/// AZ-7：进行中的 Azure HTTP 请求必须能被 RecognitionCancellation 主动中断。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM = 20 ms × 16,000 × 2 = 640 bytes
///   server response delay = 2 s（大于取消验收 1 s）
///   client timeout = 30 s（取消必须早于全局超时）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. pipeline 以 User 原因提交 20ms PCM，fake server 接受后阻塞
///   2. 测试从 session 取消句柄发出取消
///   3. pipeline 丢弃 HTTP future 并收集终态
///
/// 预期结果：
///   - 正断言：1 秒内得到 InputEnded(User) + Cancelled
///   - 负断言：不得等待 2 秒响应，也不得产生 Failed/Final
#[test]
fn azure_inflight_request_is_cancelled_before_server_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Azure 应绑定临时端口");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("fake Azure 应暴露本地地址")
    );
    let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fake Azure 应接受请求");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request_is_complete(&request) {
            let count = stream.read(&mut buffer).expect("fake Azure 应读取请求");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
        }
        accepted_tx.send(()).expect("应通知测试请求已进入服务端");
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
    let config = AzureSpeechConfig::for_test_endpoint(
        endpoint,
        "zh-CN",
        AzureSpeechSecret::new("fake-cancel-key"),
    );
    let recognizer = AzureSpeechRecognizer::new(AzureSpeechClient::new(config));
    let pipeline = RecognitionPipeline::new(AsrProviderCapabilities::new(false, Some(60)));
    let session = pipeline.start(recognizer);
    let pcm =
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640]).expect("20ms PCM 应构造成功");

    session.push_pcm(pcm).expect("pipeline 应接受 20ms PCM");
    session
        .finish(InputEndReason::User)
        .expect("活跃 session 应提交 User 结束原因");
    accepted_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("Azure 请求应在 1 秒内到达 fake server");
    let cancelled_at = std::time::Instant::now();
    session.cancellation().cancel();
    let events = session.collect_events();
    let cancellation_elapsed = cancelled_at.elapsed();
    server.join().expect("fake Azure server 应正常结束");

    assert_eq!(
        events,
        vec![
            RecognitionEvent::InputEnded(InputEndReason::User),
            RecognitionEvent::Cancelled,
        ],
        "取消应成为唯一终态，不得产生迟到 Final/Failed"
    );
    assert!(
        cancellation_elapsed < std::time::Duration::from_secs(1),
        "取消应早于 2 秒服务端响应，实际耗时 {cancellation_elapsed:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            RecognitionEvent::Failed(_) | RecognitionEvent::Final(_)
        )),
        "取消会话不得暴露迟到 Failed 或 Final"
    );
}

fn spawn_fake_response(
    status: &'static str,
    body: &'static str,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Azure 应绑定临时端口");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("fake Azure 应暴露本地地址")
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fake Azure 应接受一次请求");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request_is_complete(&request) {
            let count = stream.read(&mut buffer).expect("fake Azure 应读取请求");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
        }
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        )
        .expect("fake Azure 应写出响应");
    });
    (endpoint, server)
}

fn request_is_complete(bytes: &[u8]) -> bool {
    let request = String::from_utf8_lossy(bytes);
    let Some(header_end) = request.find("\r\n\r\n") else {
        return false;
    };
    let content_length = request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    bytes.len().saturating_sub(header_end + 4) >= content_length
}
