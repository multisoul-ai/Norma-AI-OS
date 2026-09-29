#![cfg(target_os = "macos")]

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

/// L3：本机 Keychain 中的凭据应能建立真实 Seeduplex 3.0 工具会话。
///
/// 数据构造（含关键数值的推导过程）：16 kHz PCM 输入、24 kHz PCM 输出来自协议；
/// 15 秒是网络握手上限，工具数为 1 个只读声明，不上传麦克风音频。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Keychain 读取本机测试凭据，建立官方 WSS 连接。
///   2. 发送包含 Function Calling 定义的 session.create。
///   3. 等待 session.created 或 provider error，随后关闭会话。
///
/// 预期结果：会话建立且不返回权限错误；密钥、完整服务原文和音频均不写入日志。
#[tokio::test]
#[ignore = "需要本机 Keychain 凭据和 Seeduplex 3.0 账号授权"]
async fn doubao_real_function_session_entitlement() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let credential = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "com.norma-ai.os.agent-provider",
            "-a",
            "doubao-ark-e2e",
            "-w",
        ])
        .output()
        .expect("应可启动本机 Keychain 读取命令");
    assert!(
        credential.status.success(),
        "本机 Keychain 应有可读取的隔离测试凭据"
    );
    let secret = String::from_utf8(credential.stdout).expect("测试凭据应为 UTF-8");
    let secret = secret.trim();
    let mut request = "wss://openspeech.bytedance.com/api/v3/duplex/realtime/dialogue"
        .into_client_request()
        .expect("官方 URL 应有效");
    request.headers_mut().insert(
        "X-Api-Key",
        HeaderValue::from_str(&secret).expect("凭据应能写入请求头"),
    );
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .expect("真实豆包连接应在 15 秒内完成")
    .expect("真实豆包 TLS/WebSocket 握手应成功");
    let config = json!({"type":"session.create","session":{"model":"1.2.6.1","instructions":"你是 Norma，请简短回答。",
        "audio":{"input":{"format":{"type":"pcm","rate":16000}},"output":{"format":{"type":"pcm","rate":24000},"voice":"zh_female_vv_jupiter_bigtts"}},
        "tools":[{"type":"function","name":"find_agent_tasks","description":"查看任务","parameters":{"type":"object","properties":{},"additionalProperties":false}}]},
        "extension":{"tts":{"audio_config":{"format":"pcm_s16le","sample_rate":24000,"channel":1}}}});
    socket
        .send(Message::Text(config.to_string().into()))
        .await
        .expect("应发送真实会话配置");
    let message = tokio::time::timeout(Duration::from_secs(15), socket.next())
        .await
        .expect("服务应在 15 秒内确认或拒绝会话")
        .expect("服务不应无回执关闭连接")
        .expect("服务消息应有效");
    let event: Value = serde_json::from_str(message.to_text().expect("豆包控制事件应为 JSON 文本"))
        .expect("豆包控制事件应能解析");
    let event_type = event["type"].as_str().unwrap_or("missing_type");
    let code = event["error"]["code"].as_str().unwrap_or("none");
    println!("doubao_event_type={event_type} error_code={code}");
    assert_eq!(
        event_type, "session.created",
        "本机账号应获得 Seeduplex 3.0 Function Calling 会话，而非错误事件"
    );
    assert_ne!(event_type, "error", "提供的凭据不能被拒绝为缺少全双工授权");
    let _ = socket
        .send(Message::Text(
            json!({"type":"session.close"}).to_string().into(),
        ))
        .await;
}
