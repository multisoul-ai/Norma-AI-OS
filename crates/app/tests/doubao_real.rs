#![cfg(target_os = "macos")]

use flate2::{write::GzEncoder, Compression};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{io::Write, time::Duration};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

/// L3：同一把本机方舟 Key 应通过 ASR 2.0、方舟 Chat 2.0 与 TTS 2.0 的真实服务。
///
/// 数据构造（含关键数值的推导过程）：ASR 配置声明 16 kHz × 16 bit × 1 声道；
/// TTS 请求声明 24 kHz PCM，测试文字为两个汉字，未上传麦克风数据。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从本机 Keychain 读取用户提供的方舟 Key，不写入日志。
///   2. 与 ASR 2.0 WebSocket 握手并收取配置确认。
///   3. 调用方舟 Chat 2.0 获取短回复，再请求 TTS 2.0 的音频流。
///
/// 预期结果：三个真实服务均接受同一凭据；不再依赖 Seeduplex 3.0 鉴权。
#[tokio::test]
#[ignore = "需要本机 Keychain 方舟凭据与联网，消耗少量方舟额度"]
async fn doubao_real_ark_2_services() {
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
        .expect("应可读取本机 Keychain");
    assert!(
        credential.status.success(),
        "本机 Keychain 应有方舟测试凭据"
    );
    let key = String::from_utf8(credential.stdout).expect("凭据应为 UTF-8");
    let key = key.trim();
    let mut request = "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async"
        .into_client_request()
        .expect("ASR 2.0 URL 应有效");
    request.headers_mut().insert(
        "X-Api-Key",
        HeaderValue::from_str(key).expect("Key 应可入头"),
    );
    request.headers_mut().insert(
        "X-Api-Resource-Id",
        HeaderValue::from_static("volc.seedasr.sauc.duration"),
    );
    request.headers_mut().insert(
        "X-Api-Request-Id",
        HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).expect("请求 ID 应有效"),
    );
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .expect("ASR 握手不应超时")
    .expect("ASR 2.0 应接受方舟 Key");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(
            json!({"audio":{"format":"pcm","codec":"raw","rate":16000,"bits":16,"channel":1},
        "request":{"model_name":"bigmodel","show_utterances":true,"enable_nonstream":true,
        "result_type":"single","end_window_size":500}})
            .to_string()
            .as_bytes(),
        )
        .expect("应编码 ASR 配置");
    let payload = encoder.finish().expect("应完成 ASR gzip 帧");
    let mut frame = vec![0x11, 0x11, 0x11, 0];
    frame.extend_from_slice(&1_i32.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    socket
        .send(Message::Binary(frame.into()))
        .await
        .expect("应发送 ASR 配置");
    let ack = tokio::time::timeout(Duration::from_secs(15), socket.next())
        .await
        .expect("ASR 配置不应超时")
        .expect("ASR 应回复配置")
        .expect("ASR 帧应有效");
    assert!(
        matches!(ack, Message::Binary(_)),
        "ASR 2.0 应返回二进制确认帧"
    );
    assert!(
        !matches!(ack, Message::Text(_)),
        "不应收到 Seeduplex JSON 控制事件"
    );
    drop(socket);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("应创建 HTTP 客户端");
    let chat = client.post("https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions")
        .bearer_auth(key).json(&json!({"model":"doubao-seed-2.0-mini",
            "messages":[{"role":"user","content":"你好"}],"max_tokens":64,"thinking":{"type":"disabled"}}))
        .send().await.expect("方舟 Chat 应可连接");
    assert!(chat.status().is_success(), "方舟 Chat 2.0 应接受同一把 Key");
    let chat: Value = chat.json().await.expect("方舟 Chat 应返回 JSON");
    assert!(
        chat["choices"][0]["message"]["content"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "方舟 Chat 应返回非空回复"
    );
    assert_ne!(
        chat["choices"][0]["finish_reason"], "error",
        "方舟回复不应失败"
    );

    let tts = client
        .post("https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional")
        .header("X-Api-Key", key)
        .header("X-Api-Resource-Id", "seed-tts-2.0")
        .header("X-Api-Request-Id", uuid::Uuid::new_v4().to_string())
        .json(
            &json!({"req_params":{"text":"你好。","speaker":"zh_female_vv_uranus_bigtts",
            "audio_params":{"format":"pcm","sample_rate":24000}}}),
        )
        .send()
        .await
        .expect("TTS 2.0 应可连接");
    assert!(tts.status().is_success(), "TTS 2.0 应接受同一把 Key");
    let text = tts.text().await.expect("TTS 音频流应可读取");
    let has_audio = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|record| record["data"].as_str().is_some_and(|data| !data.is_empty()));
    assert!(has_audio, "TTS 2.0 应返回非空 PCM 音频");
    assert!(!text.is_empty(), "TTS 响应不应为空");
}
