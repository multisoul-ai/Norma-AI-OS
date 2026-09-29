use async_channel::Sender;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::io::{Read, Write};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
use tokio_util::sync::CancellationToken;

use super::{plan_url, request_id};

pub(super) enum SpeechEvent {
    Ready,
    Started,
    Final(String),
}

pub(crate) fn encode_start() -> Result<Vec<u8>, String> {
    encode(
        0x11,
        1,
        &serde_json::to_vec(&json!({
            "audio":{"format":"pcm","codec":"raw","rate":16000,"bits":16,"channel":1},
            "request":{"model_name":"bigmodel","show_utterances":true,
                "enable_nonstream":true,"result_type":"single","end_window_size":500}
        }))
        .map_err(|_| "无法编码 ASR 配置".to_string())?,
    )
}

pub(crate) fn encode_audio(sequence: i32, pcm: &[u8]) -> Result<Vec<u8>, String> {
    encode(0x21, sequence, pcm)
}

fn encode(kind: u8, sequence: i32, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(data)
        .map_err(|_| "无法压缩 ASR 音频".to_string())?;
    let payload = gzip
        .finish()
        .map_err(|_| "无法完成 ASR 音频帧".to_string())?;
    let size = u32::try_from(payload.len()).map_err(|_| "ASR 帧过大".to_string())?;
    let mut frame = Vec::with_capacity(12 + payload.len());
    frame.extend_from_slice(&[0x11, kind, 0x11, 0]);
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&size.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn decode(raw: &[u8]) -> Result<Value, String> {
    if raw.len() < 12 || raw[0] >> 4 != 1 || raw[2] >> 4 != 1 {
        return Err("ASR 返回了无效帧头".into());
    }
    let kind = raw[1] >> 4;
    if kind != 9 && kind != 15 {
        return Err("ASR 返回了未知帧类型".into());
    }
    let flags = raw[1] & 15;
    let mut offset = usize::from(raw[0] & 15) * 4;
    if flags & 1 != 0 {
        offset += 4;
    }
    if flags & 4 != 0 {
        offset += 4;
    }
    if kind == 15 {
        offset += 4;
    }
    let len = raw
        .get(offset..offset + 4)
        .and_then(|part| <[u8; 4]>::try_from(part).ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| "ASR 帧长度缺失".to_string())? as usize;
    offset += 4;
    if len > 2 * 1024 * 1024 || raw.len() != offset + len {
        return Err("ASR 响应长度无效".into());
    }
    let mut payload = Vec::new();
    match raw[2] & 15 {
        0 => payload.extend_from_slice(&raw[offset..]),
        1 => {
            GzDecoder::new(&raw[offset..])
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut payload)
                .map_err(|_| "ASR 响应解压失败".to_string())?;
        }
        _ => return Err("ASR 响应压缩格式无效".into()),
    }
    if payload.len() > 2 * 1024 * 1024 {
        return Err("ASR 响应过大".into());
    }
    let value: Value =
        serde_json::from_slice(&payload).map_err(|_| "ASR 响应 JSON 无效".to_string())?;
    if kind == 15
        || value["code"]
            .as_i64()
            .is_some_and(|code| code != 0 && code != 1000 && code != 20_000_000)
    {
        return Err("ASR 2.0 服务拒绝了请求；请检查方舟权限".into());
    }
    Ok(value)
}

pub(super) async fn run(
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
) -> Result<(), String> {
    let url = plan_url(
        "NORMA_DOUBAO_ASR_URL",
        "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async",
    );
    let mut request = url
        .into_client_request()
        .map_err(|_| "ASR 连接地址无效".to_string())?;
    for (name, value) in [
        ("X-Api-Key", key.as_str()),
        ("X-Api-Resource-Id", "volc.seedasr.sauc.duration"),
    ] {
        request.headers_mut().insert(
            name,
            HeaderValue::from_str(value).map_err(|_| "ASR 鉴权字段无效".to_string())?,
        );
    }
    request.headers_mut().insert(
        "X-Api-Request-Id",
        HeaderValue::from_str(&request_id()).map_err(|_| "ASR 请求 ID 无效".to_string())?,
    );
    let (mut socket, _) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "ASR 连接超时".to_string())?
    .map_err(|error| super::super::connect_failure_message(&error).to_string())?;
    socket
        .send(Message::Binary(encode_start()?.into()))
        .await
        .map_err(|_| "ASR 配置发送失败".to_string())?;
    let mut capture = None;
    let mut pcm_rx: Option<async_channel::Receiver<audio::PcmAudioChunk>> = None;
    let mut pcm_buffer = Vec::new();
    let mut sequence = 2;
    let mut ready = false;
    let mut speaking = false;
    let mut last_final = -1_i64;
    let mut final_text = String::new();
    let handshake_deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(handshake_deadline);
    let result = loop {
        tokio::select! {
            _ = cancellation.cancelled() => break Ok(()),
            _ = &mut handshake_deadline, if !ready => break Err("ASR 2.0 配置确认超时".into()),
            received = socket.next() => {
                let Some(received) = received else { break Err("ASR 连接已断开".into()); };
                let message = received.map_err(|_| "ASR 连接已断开".to_string())?;
                let Message::Binary(raw) = message else { continue; };
                let payload = decode(&raw)?;
                if !ready {
                    ready = true;
                    let (session, chunks) = tokio::task::spawn_blocking(|| voice_input::start_duplex_pcm_stream(None))
                        .await.map_err(|_| "麦克风启动失败".to_string())?
                        .map_err(|error| format!("麦克风启动失败：{error}"))?;
                    capture = Some(session);
                    pcm_rx = Some(chunks);
                    let _ = events.send(SpeechEvent::Ready).await;
                }
                let Some(utterances) = payload["result"]["utterances"].as_array() else { continue; };
                if utterances.len() > 128 { break Err("ASR 返回过多分句".into()); }
                let mut committed = String::new();
                let mut interim = String::new();
                for utterance in utterances {
                    let Some(text) = utterance["text"].as_str().filter(|text| !text.trim().is_empty()) else { continue; };
                    let Some(end) = utterance["end_time"].as_i64().filter(|end| *end > last_final) else { continue; };
                    if utterance["definite"] == true {
                        committed.push_str(text);
                        last_final = end;
                    } else {
                        interim.push_str(text);
                    }
                }
                if committed.is_empty() && interim.is_empty() { continue; }
                if !speaking {
                    speaking = true;
                    let _ = events.send(SpeechEvent::Started).await;
                }
                final_text.push_str(&committed);
                if final_text.len() + interim.len() > 2_000 { break Err("ASR 转写过长".into()); }
                if interim.is_empty() && !final_text.is_empty() {
                    speaking = false;
                    let _ = events.send(SpeechEvent::Final(std::mem::take(&mut final_text))).await;
                }
            }
            chunk = async { pcm_rx.as_ref().expect("ready capture").recv().await }, if pcm_rx.is_some() => {
                let Ok(chunk) = chunk else { break Err("麦克风采集已停止".into()); };
                pcm_buffer.extend_from_slice(&chunk.bytes);
                // 5 × 20 ms × 16 kHz × 2 B = 3,200 B：低延迟且落在官方 100–200 ms 建议范围。
                while pcm_buffer.len() >= 3_200 {
                    let frame: Vec<u8> = pcm_buffer.drain(..3_200).collect();
                    socket.send(Message::Binary(encode_audio(sequence, &frame)?.into())).await
                        .map_err(|_| "ASR 音频上传中断".to_string())?;
                    sequence += 1;
                }
            }
        }
    };
    drop(pcm_rx);
    if let Some(capture) = capture {
        let _ = tokio::task::spawn_blocking(move || drop(capture)).await;
    }
    result
}
