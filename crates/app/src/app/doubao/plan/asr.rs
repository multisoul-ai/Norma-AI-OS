use async_channel::Sender;
use flate2::{write::GzEncoder, Compression};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::io::Write;
use std::time::Instant;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

use super::{plan_url, request_id};

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_hold_asr_tests.rs"]
mod hold_tests;
mod listen;
mod protocol;
#[cfg(test)]
use listen::listen_recordings_at;
pub(in crate::app::doubao) use listen::{listen_continuous, listen_recordings};
pub(crate) use protocol::decode;

pub(in crate::app::doubao) enum SpeechEvent {
    Ready,
    Started,
    HeldStarted(u64),
    HeldReady(u64),
    HeldFinal(u64, String),
    Final(String),
    /// 一次收音（按住录音或持续聆听）结束；`Some` 表示识别或麦克风失败。
    ListenEnded(u64, Option<LocalizedText>),
}

pub(crate) fn encode_start() -> Result<Vec<u8>, LocalizedText> {
    encode_start_for(false)
}

fn encode_start_for(held: bool) -> Result<Vec<u8>, LocalizedText> {
    encode(
        0x11,
        1,
        &serde_json::to_vec(&json!({
            "audio":{"format":"pcm","codec":"raw","rate":16000,"bits":16,"channel":1},
            "request":{"model_name":"bigmodel","show_utterances":true,
                "enable_nonstream":true,"result_type":if held { "full" } else { "single" },"end_window_size":500}
        }))
        .map_err(|_| LocalizedText::new("doubao.asr_config_encode"))?,
    )
}

pub(crate) fn encode_audio(sequence: i32, pcm: &[u8]) -> Result<Vec<u8>, LocalizedText> {
    encode(0x21, sequence, pcm)
}

fn encode(kind: u8, sequence: i32, data: &[u8]) -> Result<Vec<u8>, LocalizedText> {
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(data)
        .map_err(|_| LocalizedText::new("doubao.asr_compress"))?;
    let payload = gzip
        .finish()
        .map_err(|_| LocalizedText::new("doubao.asr_frame_finish"))?;
    let size =
        u32::try_from(payload.len()).map_err(|_| LocalizedText::new("doubao.asr_frame_large"))?;
    let mut frame = Vec::with_capacity(12 + payload.len());
    frame.extend_from_slice(&[0x11, if sequence < 0 { kind | 2 } else { kind }, 0x11, 0]);
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&size.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

#[cfg(test)]
async fn run(
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    input: Option<super::super::input::RecordingInput>,
) -> Result<(), LocalizedText> {
    let url = plan_url(
        "NORMA_DOUBAO_ASR_URL",
        "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async",
    );
    run_at(url, key, events, cancellation, input).await
}

async fn run_at(
    url: String,
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    input: Option<super::super::input::RecordingInput>,
) -> Result<(), LocalizedText> {
    let held = input.is_some();
    let (recording_id, mut input) = match input {
        Some((id, input)) => (id, Some(input)),
        None => (0, None),
    };
    let started_at = Instant::now();
    let request_id = request_id();
    let mut request = url
        .into_client_request()
        .map_err(|_| LocalizedText::new("doubao.asr_url_invalid"))?;
    for (name, value) in [
        ("X-Api-Key", key.as_str()),
        ("X-Api-Resource-Id", "volc.seedasr.sauc.duration"),
    ] {
        request.headers_mut().insert(
            name,
            HeaderValue::from_str(value)
                .map_err(|_| LocalizedText::new("doubao.asr_auth_invalid"))?,
        );
    }
    request.headers_mut().insert(
        "X-Api-Request-Id",
        HeaderValue::from_str(&request_id)
            .map_err(|_| LocalizedText::new("doubao.asr_id_invalid"))?,
    );
    tracing::info!(target: "norma_voice", %request_id, "ASR WebSocket connecting");
    let connection = tokio::select! {
        _ = cancellation.cancelled() => {
            tracing::info!(target: "norma_voice", %request_id,
                elapsed_ms = started_at.elapsed().as_millis(), "ASR connect cancelled");
            return Ok(());
        },
        result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async(request),
        ) => result,
    };
    let (mut socket, response) = match connection {
        Ok(Ok(connected)) => connected,
        Ok(Err(error)) => {
            let reason = super::super::connect_failure_message(&error);
            tracing::warn!(target: "norma_voice", %request_id,
                elapsed_ms = started_at.elapsed().as_millis(), %reason,
                "ASR WebSocket connection failed");
            return Err(reason);
        }
        Err(_) => {
            tracing::warn!(target: "norma_voice", %request_id,
                elapsed_ms = started_at.elapsed().as_millis(), "ASR WebSocket connection timed out");
            return Err(LocalizedText::new("doubao.asr_connect_timeout"));
        }
    };
    let provider_logid = response
        .headers()
        .get("x-tt-logid")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    tracing::info!(target: "norma_voice", %request_id, %provider_logid,
        connect_ms = started_at.elapsed().as_millis(), "ASR WebSocket connected");
    socket
        .send(Message::Binary(
            if held {
                encode_start_for(true)?
            } else {
                encode_start()?
            }
            .into(),
        ))
        .await
        .map_err(|_| LocalizedText::new("doubao.asr_config_send"))?;
    let mut capture = None;
    let mut pcm_rx: Option<async_channel::Receiver<audio::PcmAudioChunk>> = None;
    let mut pcm_buffer = Vec::new();
    let mut sequence = 2;
    let mut ready = false;
    let mut last_frame_sent = false;
    let mut speaking = false;
    let mut last_final = -1_i64;
    let mut final_text = String::new();
    let mut final_deadline = None;
    let handshake_deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(handshake_deadline);
    let result = async {
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break Ok(()),
                _ = async { tokio::time::sleep_until(final_deadline.expect("结束帧已发送")).await }, if final_deadline.is_some() => break Err(LocalizedText::new("doubao.asr_final_timeout")),
                _ = &mut handshake_deadline, if !ready => break Err(LocalizedText::new("doubao.asr_config_timeout")),
                received = socket.next() => {
                    let Some(received) = received else { break Err(LocalizedText::new("doubao.asr_disconnected")); };
                    let message = received.map_err(|_| LocalizedText::new("doubao.asr_disconnected"))?;
                    let Message::Binary(raw) = message else { continue; };
                    let payload = decode(&raw).map_err(|error| {
                        tracing::warn!(target: "norma_voice", %request_id, %provider_logid,
                            audio_frames = sequence - 2, elapsed_ms = started_at.elapsed().as_millis(),
                            reason = %error, "ASR response rejected");
                        error
                    })?;
                    if !ready {
                        ready = true;
                        if !held {
                            let (session, chunks) = tokio::task::spawn_blocking(|| voice_input::start_duplex_pcm_stream(None))
                            .await.map_err(|_| LocalizedText::new("doubao.microphone_start"))?
                            .map_err(|error| LocalizedText::new("doubao.microphone_start_reason").with_arg("reason", error))?;
                            capture = Some(session);
                            pcm_rx = Some(chunks);
                        }
                        tracing::info!(target: "norma_voice", %request_id, %provider_logid,
                            ready_ms = started_at.elapsed().as_millis(), "ASR configuration accepted and microphone ready");
                        if !held { let _ = events.send(SpeechEvent::Ready).await; }
                    }
                    if held && raw[1] & 2 != 0 {
                        if !last_frame_sent { break Err(LocalizedText::new("doubao.asr_ended_before_release")); }
                        // 上游最终结果的 text 是整段转写，包含松键时尚未 definite 的尾音。
                        if let Some(text) = payload["result"]["text"].as_str().filter(|text| !text.trim().is_empty()) {
                            final_text = text.to_owned();
                        } else if let Some(utterances) = payload["result"]["utterances"].as_array() {
                            for utterance in utterances {
                                if utterance["end_time"].as_i64().is_some_and(|end| end > last_final) {
                                    if let Some(text) = utterance["text"].as_str() { final_text.push_str(text); }
                                }
                            }
                        }
                        if final_text.len() > 2_000 { break Err(LocalizedText::new("doubao.asr_transcript_large")); }
                        if final_text.trim().is_empty() {
                            let _ = events.send(SpeechEvent::HeldReady(recording_id)).await;
                        } else {
                            let _ = events.send(SpeechEvent::HeldFinal(recording_id, std::mem::take(&mut final_text))).await;
                        }
                        break Ok(());
                    }
                    let Some(utterances) = payload["result"]["utterances"].as_array() else { continue; };
                    if utterances.len() > 128 { break Err(LocalizedText::new("doubao.asr_utterances_large")); }
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
                    if !held && !speaking {
                        speaking = true;
                        let _ = events.send(SpeechEvent::Started).await;
                    }
                    final_text.push_str(&committed);
                    if final_text.len() + interim.len() > 2_000 { break Err(LocalizedText::new("doubao.asr_transcript_large")); }
                    if !held && interim.is_empty() && !final_text.is_empty() {
                        speaking = false;
                        let _ = events.send(SpeechEvent::Final(std::mem::take(&mut final_text))).await;
                    }
                }
                chunk = async { input.as_ref().expect("按住录音输入").recv().await }, if ready && input.is_some() => {
                    match chunk {
                        Ok(Ok(chunk)) => pcm_buffer.extend_from_slice(&chunk.bytes),
                        Ok(Err(error)) => break Err(error),
                        Err(_) => {
                            input = None;
                            if pcm_buffer.is_empty() { pcm_buffer.resize(3_200, 0); }
                            socket.send(Message::Binary(encode_audio(-sequence, &pcm_buffer)?.into())).await
                                .map_err(|_| LocalizedText::new("doubao.asr_upload_interrupted"))?;
                            pcm_buffer.clear();
                            last_frame_sent = true;
                            // 与握手相同的 10 s 上限，避免松键后永久等待最终结果。
                            final_deadline = Some(tokio::time::Instant::now() + std::time::Duration::from_secs(10));
                            continue;
                        }
                    }
                    while pcm_buffer.len() >= 3_200 {
                        let frame: Vec<u8> = pcm_buffer.drain(..3_200).collect();
                        socket.send(Message::Binary(encode_audio(sequence, &frame)?.into())).await
                            .map_err(|_| LocalizedText::new("doubao.asr_upload_interrupted"))?;
                        sequence += 1;
                    }
                }
                chunk = async { pcm_rx.as_ref().expect("ready capture").recv().await }, if pcm_rx.is_some() => {
                    let Ok(chunk) = chunk else { break Err(LocalizedText::new("doubao.microphone_stopped")); };
                    pcm_buffer.extend_from_slice(&chunk.bytes);
                    // 5 × 20 ms × 16 kHz × 2 B = 3,200 B：低延迟且落在官方 100–200 ms 建议范围。
                    while pcm_buffer.len() >= 3_200 {
                        let frame: Vec<u8> = pcm_buffer.drain(..3_200).collect();
                        socket.send(Message::Binary(encode_audio(sequence, &frame)?.into())).await
                            .map_err(|_| LocalizedText::new("doubao.asr_upload_interrupted"))?;
                        if sequence == 2 {
                            tracing::info!(target: "norma_voice", %request_id, %provider_logid,
                                first_audio_ms = started_at.elapsed().as_millis(),
                                "first ASR audio frame uploaded");
                        }
                        sequence += 1;
                    }
                }
            }
        }
    }
    .await;
    drop(pcm_rx);
    if let Some(capture) = capture {
        let _ = tokio::task::spawn_blocking(move || drop(capture)).await;
    }
    let mut close_wait_timed_out = false;
    if cancellation.is_cancelled() {
        if ready && !last_frame_sent {
            // 最后一帧用负序号通知 ASR 释放会话；没有余音时发 100 ms 静音。
            if pcm_buffer.is_empty() {
                pcm_buffer.resize(3_200, 0);
            }
            if let Ok(frame) = encode_audio(-sequence, &pcm_buffer) {
                last_frame_sent = socket.send(Message::Binary(frame.into())).await.is_ok();
            }
        }
        let _ = socket.close(None).await;
        close_wait_timed_out = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(message) = socket.next().await {
                if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                    break;
                }
            }
        })
        .await
        .is_err();
    } else {
        let _ = socket.close(None).await;
    }
    tracing::info!(target: "norma_voice", %request_id, %provider_logid,
        audio_frames = sequence - 2, last_frame_sent, cancelled = cancellation.is_cancelled(),
        close_wait_timed_out, reason = result.as_ref().err().map(LocalizedText::as_str).unwrap_or("normal"),
        elapsed_ms = started_at.elapsed().as_millis(), "ASR session ended");
    result
}
