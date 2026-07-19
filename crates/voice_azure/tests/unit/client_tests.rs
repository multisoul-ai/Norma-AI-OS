use crate::{AzureSpeechClient, AzureSpeechConfig};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn start_fake_azure(response_body: &'static str) -> (String, Arc<Mutex<Vec<u8>>>, JoinHandle<()>) {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("fake Azure Speech listener 应绑定本地临时端口");
    let endpoint = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("fake Azure Speech listener 应能读取本地地址")
    );
    let request = Arc::new(Mutex::new(Vec::new()));
    let request_for_thread = Arc::clone(&request);
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("fake Azure Speech 应接受一次识别请求");
        let bytes = read_http_request(&mut stream);
        *request_for_thread
            .lock()
            .expect("fake Azure Speech request lock 不应 poisoned") = bytes;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("fake Azure Speech 应写出成功响应");
    });
    (endpoint, request, handle)
}

fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("fake Azure Speech stream 应设置读取超时");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if http_request_complete(&bytes) {
                    break;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("fake Azure Speech 读取请求失败: {error}"),
        }
    }
    bytes
}

fn http_request_complete(bytes: &[u8]) -> bool {
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

/// AZ-STT-1：短音频识别成功时，应发送 Azure 要求的请求并返回显示文本。
///
/// 数据构造（含关键数值的推导过程）：
///   WAV fixture = 12 bytes（RIFF/WAVE 标识各 4 bytes，共 4 + 4 + 4 = 12 bytes）
///   locale = zh-CN
///   fake Azure response = RecognitionStatus(Success) + DisplayText("打开一个终端。")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. client 向 conversation/cognitiveservices/v1 发起 POST
///   2. client 带上 zh-CN、订阅密钥与 16 kHz PCM/WAV Content-Type
///   3. fake Azure 返回成功 JSON，client 解析 DisplayText
///
/// 预期结果：
///   - 正断言：结果文本为“打开一个终端。”
///   - 正断言：请求包含正确路径、locale、密钥、Content-Type 与完整 WAV body
///   - 负断言：请求不得把订阅密钥放入 URL query
#[test]
fn test_transcribe_short_wav_sends_azure_contract_and_returns_display_text() {
    let response = r#"{"RecognitionStatus":"Success","DisplayText":"打开一个终端。","Offset":0,"Duration":12}"#;
    let (endpoint, request, handle) = start_fake_azure(response);
    let config = AzureSpeechConfig::new(endpoint, "fake-unit-test-key", "zh-CN");
    let client = AzureSpeechClient::new(config);
    let wav_bytes = b"RIFF----WAVE";

    let recognition = client
        .transcribe_short_wav(wav_bytes)
        .expect("fake Azure 返回 Success 时短音频识别应成功");
    handle
        .join()
        .expect("fake Azure Speech server thread 应正常结束");

    assert_eq!(
        recognition.display_text, "打开一个终端。",
        "Success 响应应返回 Azure 的 DisplayText"
    );
    let request = request
        .lock()
        .expect("fake Azure Speech request lock 不应 poisoned");
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("Azure Speech HTTP 请求应包含 header/body 分隔符");
    let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
    assert!(
        headers.starts_with(
            "post /speech/recognition/conversation/cognitiveservices/v1?language=zh-cn http/1.1"
        ),
        "请求应 POST 到短音频 conversation endpoint，并携带 zh-CN locale；实际 headers={headers}"
    );
    assert!(
        headers.contains("ocp-apim-subscription-key: fake-unit-test-key"),
        "订阅密钥应只通过 Ocp-Apim-Subscription-Key header 发送"
    );
    assert!(
        headers.contains("content-type: audio/wav; codecs=audio/pcm; samplerate=16000"),
        "请求应声明 Azure 短音频 REST 要求的 16 kHz PCM/WAV Content-Type"
    );
    assert!(
        !headers
            .lines()
            .next()
            .unwrap_or_default()
            .contains("fake-unit-test-key"),
        "订阅密钥不得出现在请求 URL query 中"
    );
    assert_eq!(
        &request[header_end + 4..],
        wav_bytes,
        "Azure Speech 请求 body 应逐字节保留输入 WAV"
    );
}

/// AZ-STT-2：区域配置应生成专用 STT Endpoint，且调试文本不得泄露订阅密钥。
///
/// 数据构造（含关键数值的推导过程）：
///   region = eastus（Azure Portal 的资源位置）
///   expected endpoint = https:// + eastus + .stt.speech.microsoft.com
///   fake key = 20 个可辨识 ASCII 字符，仅用于检测日志脱敏
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用 region、fake key、zh-CN 构造 AzureSpeechConfig
///   2. 读取识别 Endpoint，并格式化 Debug 文本
///   3. 比较识别域名与敏感字段的可见性
///
/// 预期结果：
///   - 正断言：Endpoint 使用 eastus.stt.speech.microsoft.com
///   - 正断言：Debug 文本显示订阅密钥已脱敏
///   - 负断言：Endpoint 不得使用 eastus.api.cognitive.microsoft.com 管理域名
///   - 负断言：Debug 文本不得包含 fake key
#[test]
fn test_region_config_uses_stt_endpoint_and_redacts_subscription_key() {
    let config = AzureSpeechConfig::for_region("eastus", "fake-sensitive-key", "zh-CN")
        .expect("eastus 应是合法 Azure Speech region identifier");

    assert_eq!(
        config.endpoint(),
        "https://eastus.stt.speech.microsoft.com",
        "区域配置应生成 Azure 短音频识别专用 STT Endpoint"
    );
    assert!(
        !config.endpoint().contains("api.cognitive.microsoft.com"),
        "短音频识别不得误用 Azure Speech 管理 API Endpoint"
    );
    let debug = format!("{config:?}");
    assert!(
        debug.contains("[REDACTED]"),
        "配置 Debug 文本应明确显示订阅密钥已脱敏"
    );
    assert!(
        !debug.contains("fake-sensitive-key"),
        "配置 Debug 文本不得泄露 Azure Speech 订阅密钥"
    );
}

/// AZ-STT-3：Azure 返回 Success 但没有文本时，应拒绝伪成功结果。
///
/// 数据构造（含关键数值的推导过程）：
///   WAV fixture = 12 bytes（RIFF/WAVE 标识各 4 bytes，共 4 + 4 + 4 = 12 bytes）
///   RecognitionStatus = Success
///   DisplayText = 3 个空白字符，trim 后长度 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. client 上传 WAV fixture
///   2. fake Azure 返回 Success 与纯空白 DisplayText
///   3. client 校验识别文本是否包含有效内容
///
/// 预期结果：
///   - 正断言：返回明确的 empty transcript 错误
///   - 负断言：不得把纯空白 DisplayText 包装成成功结果
#[test]
fn test_transcribe_short_wav_rejects_success_with_empty_display_text() {
    let response =
        r#"{"RecognitionStatus":"Success","DisplayText":"   ","Offset":0,"Duration":12}"#;
    let (endpoint, _request, handle) = start_fake_azure(response);
    let client = AzureSpeechClient::new(AzureSpeechConfig::new(
        endpoint,
        "fake-unit-test-key",
        "zh-CN",
    ));

    let error = client
        .transcribe_short_wav(b"RIFF----WAVE")
        .expect_err("纯空白 DisplayText 不得作为成功识别结果返回");
    handle
        .join()
        .expect("fake Azure Speech server thread 应正常结束");

    assert!(
        error.to_string().contains("empty transcript"),
        "空识别结果应返回可诊断的 empty transcript 错误；实际 error={error}"
    );
    assert!(
        !error.to_string().contains("fake-unit-test-key"),
        "识别错误不得泄露 Azure Speech 订阅密钥"
    );
}
