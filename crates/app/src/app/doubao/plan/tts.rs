use base64::Engine as _;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::{plan_url, request_id};
use crate::app::doubao::playback::PlaybackQueue;
use audio::{PcmAudioChunk, PcmFormat};

pub(super) async fn speak(
    client: &Client,
    key: &str,
    text: &str,
    playback: &Arc<PlaybackQueue>,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if text.trim().is_empty() || cancellation.is_cancelled() {
        return Ok(());
    }
    let generation = playback.generation();
    let url = plan_url(
        "NORMA_DOUBAO_TTS_URL",
        "https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional",
    );
    let request = client
        .post(url)
        .header("X-Api-Key", key)
        .header("X-Api-Resource-Id", "seed-tts-2.0")
        .header("X-Api-Request-Id", request_id())
        .json(&json!({"req_params":{
            "text":text,"speaker":"zh_female_vv_uranus_bigtts",
            "audio_params":{"format":"pcm","sample_rate":24000}
        }}));
    let mut response = tokio::select! {
        _ = cancellation.cancelled() => return Ok(()),
        result = request.send() => result.map_err(|_| "豆包 TTS 请求失败".to_string())?,
    };
    if !response.status().is_success() {
        return Err("豆包 TTS 2.0 拒绝了请求；请检查方舟 Key 与资源权限".into());
    }
    let mut buffer = Vec::new();
    let mut ended = false;
    let mut bytes = 0_usize;
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            result = response.chunk() => result.map_err(|_| "豆包 TTS 流中断".to_string())?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        buffer.extend_from_slice(&chunk);
        if buffer.len() > 3 * 1024 * 1024 {
            return Err("豆包 TTS 响应过大".into());
        }
        while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=index).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            process_line(&line, &mut ended, &mut bytes, playback, generation)?;
        }
    }
    if !buffer.iter().all(u8::is_ascii_whitespace) {
        process_line(&buffer, &mut ended, &mut bytes, playback, generation)?;
    }
    if !ended || bytes == 0 {
        return Err("豆包 TTS 未返回完整音频".into());
    }
    Ok(())
}

fn process_line(
    line: &[u8],
    ended: &mut bool,
    total_bytes: &mut usize,
    playback: &PlaybackQueue,
    generation: u64,
) -> Result<(), String> {
    if *ended {
        return Err("豆包 TTS 终态后仍有数据".into());
    }
    let record: Value =
        serde_json::from_slice(line).map_err(|_| "豆包 TTS 数据无效".to_string())?;
    if record["code"] == 20_000_000 {
        *ended = true;
        return Ok(());
    }
    if record["code"] != 0 {
        return Err("豆包 TTS 合成失败".into());
    }
    let Some(encoded) = record["data"].as_str() else {
        return Ok(());
    };
    let pcm = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "豆包 TTS 音频编码无效".to_string())?;
    if pcm.is_empty() || pcm.len() % 2 != 0 {
        return Err("豆包 TTS PCM 格式无效".into());
    }
    *total_bytes += pcm.len();
    if *total_bytes > 1_440_000 {
        return Err("豆包 TTS 音频超出 30 秒上限".into());
    }
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, pcm)
        .map_err(|_| "豆包 TTS PCM 无效".to_string())?;
    playback.enqueue_for(generation, chunk)
}
