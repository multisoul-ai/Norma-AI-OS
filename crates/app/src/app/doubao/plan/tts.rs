use base64::Engine as _;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

use super::{plan_url, request_id};
use crate::app::doubao::playback::PlaybackQueue;
use audio::{PcmAudioChunk, PcmFormat};
use tts::providers::doubao::{request_body, DoubaoVoice, RESOURCE_ID};

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_tts_voice_tests.rs"]
mod voice_tests;

// 24 kHz mono PCM16 × 100 ms; small network fragments share one playback buffer.
const MIN_PLAYBACK_CHUNK_BYTES: usize = 24_000 * 2 / 10;

fn build_request(
    client: &Client,
    key: &str,
    text: &str,
    voice: &DoubaoVoice,
    url: &str,
) -> Result<reqwest::Request, reqwest::Error> {
    client
        .post(url)
        .header("X-Api-Key", key)
        .header("X-Api-Resource-Id", RESOURCE_ID)
        .header("X-Api-Request-Id", request_id())
        .json(&request_body(text, voice))
        .build()
}

pub(super) async fn speak(
    client: &Client,
    key: &str,
    text: &str,
    voice: &DoubaoVoice,
    playback: &Arc<PlaybackQueue>,
    cancellation: &CancellationToken,
) -> Result<(), LocalizedText> {
    if text.trim().is_empty() || cancellation.is_cancelled() {
        return Ok(());
    }
    let generation = playback.generation();
    let url = plan_url(
        "NORMA_DOUBAO_TTS_URL",
        "https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional",
    );
    let request = build_request(client, key, text, voice, &url)
        .map_err(|_| LocalizedText::new("doubao.tts_request"))?;
    let mut response = tokio::select! {
        _ = cancellation.cancelled() => return Ok(()),
        result = client.execute(request) => result.map_err(|_| LocalizedText::new("doubao.tts_request"))?,
    };
    if !response.status().is_success() {
        return Err(LocalizedText::new("doubao.tts_denied"));
    }
    let mut buffer = Vec::new();
    let mut ended = false;
    let mut bytes = 0_usize;
    let mut pending_pcm = Vec::new();
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            result = response.chunk() => result.map_err(|_| LocalizedText::new("doubao.tts_interrupted"))?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        buffer.extend_from_slice(&chunk);
        if buffer.len() > 3 * 1024 * 1024 {
            return Err(LocalizedText::new("doubao.tts_response_large"));
        }
        while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=index).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            process_line(
                &line,
                &mut ended,
                &mut bytes,
                &mut pending_pcm,
                playback,
                generation,
            )?;
        }
    }
    if !buffer.iter().all(u8::is_ascii_whitespace) {
        process_line(
            &buffer,
            &mut ended,
            &mut bytes,
            &mut pending_pcm,
            playback,
            generation,
        )?;
    }
    if !ended || bytes == 0 {
        return Err(LocalizedText::new("doubao.tts_incomplete"));
    }
    if !pending_pcm.is_empty() {
        enqueue_pcm(playback, generation, pending_pcm)?;
    }
    let finished = playback.finish_for(generation)?;
    tokio::select! {
        _ = cancellation.cancelled() => Ok(()),
        result = finished.recv() => result.map_err(|_| LocalizedText::new("doubao.playback_interrupted")),
    }
}

fn process_line(
    line: &[u8],
    ended: &mut bool,
    total_bytes: &mut usize,
    pending_pcm: &mut Vec<u8>,
    playback: &PlaybackQueue,
    generation: u64,
) -> Result<(), LocalizedText> {
    if *ended {
        return Err(LocalizedText::new("doubao.tts_after_end"));
    }
    let record: Value =
        serde_json::from_slice(line).map_err(|_| LocalizedText::new("doubao.tts_data_invalid"))?;
    if record["code"] == 20_000_000 {
        *ended = true;
        return Ok(());
    }
    if record["code"] != 0 {
        return Err(LocalizedText::new("doubao.tts_synthesis_failed"));
    }
    let Some(encoded) = record["data"].as_str() else {
        return Ok(());
    };
    let pcm = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| LocalizedText::new("doubao.tts_encoding_invalid"))?;
    if pcm.is_empty() || pcm.len() % 2 != 0 {
        return Err(LocalizedText::new("doubao.tts_pcm_format"));
    }
    *total_bytes += pcm.len();
    if *total_bytes > 1_440_000 {
        return Err(LocalizedText::new("doubao.tts_duration_limit"));
    }
    if pending_pcm.is_empty() && pcm.len() >= MIN_PLAYBACK_CHUNK_BYTES {
        return enqueue_pcm(playback, generation, pcm);
    }
    pending_pcm.extend_from_slice(&pcm);
    if pending_pcm.len() >= MIN_PLAYBACK_CHUNK_BYTES {
        enqueue_pcm(playback, generation, std::mem::take(pending_pcm))?;
    }
    Ok(())
}

fn enqueue_pcm(
    playback: &PlaybackQueue,
    generation: u64,
    pcm: Vec<u8>,
) -> Result<(), LocalizedText> {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, pcm)
        .map_err(|_| LocalizedText::new("doubao.tts_pcm_invalid"))?;
    playback.enqueue_for(generation, chunk)
}
