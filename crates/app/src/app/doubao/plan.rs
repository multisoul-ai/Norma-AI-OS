use super::{playback::start_playback, DoubaoStatus};
use agent::NormaToolRuntime;
use async_channel::{Receiver, Sender};
use reqwest::Client;
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;

pub(super) mod asr;
pub(super) mod chat;
mod tts;

fn plan_url(env: &str, production: &str) -> String {
    #[cfg(feature = "e2e-test-support")]
    {
        std::env::var(env).unwrap_or_else(|_| production.into())
    }
    #[cfg(not(feature = "e2e-test-support"))]
    {
        let _ = env;
        production.into()
    }
}

fn request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(super) async fn run_session(
    account: String,
    prompt: String,
    project_id: Option<String>,
    database: PathBuf,
    tools: Option<NormaToolRuntime>,
    cancellation: CancellationToken,
    events: Sender<DoubaoStatus>,
    reports: Receiver<String>,
) -> Result<(), String> {
    let key = tokio::task::spawn_blocking(move || {
        crate::settings::voice::VoiceSettingsState::load_credential(&account)
    })
    .await
    .map_err(|_| "无法读取方舟凭据".to_string())??
    .ok_or_else(|| "方舟 API Key 已丢失，请重新保存".to_string())?;
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| "无法初始化方舟网络客户端".to_string())?;
    let playback = Arc::new(start_playback());
    let chat = Arc::new(chat::ChatRuntime {
        client,
        key: key.clone(),
        prompt,
        history: Default::default(),
        tools: tools.map(Arc::new),
        project: project_id,
        database,
        session: request_id(),
        playback: Arc::clone(&playback),
    });
    let (speech_tx, speech_rx) = async_channel::bounded(32);
    let mut asr_task = tokio::spawn(asr::run(key, speech_tx, cancellation.clone()));
    let (done_tx, done_rx) = async_channel::unbounded::<(u64, Result<(), String>)>();
    let mut reply_token: Option<CancellationToken> = None;
    let mut reply_active = false;
    let mut reply_id = 0_u64;
    let mut turn = 0_u64;
    let result = loop {
        tokio::select! {
            _ = cancellation.cancelled() => break Ok(()),
            asr_result = &mut asr_task => {
                break asr_result.map_err(|_| "ASR 任务中断".to_string())?;
            }
            playback_error = playback.errors.recv() => {
                break Err(format!("豆包音频播放失败：{}", playback_error.unwrap_or_else(|_| "输出设备不可用".into())));
            }
            event = speech_rx.recv() => {
                match event {
                    Ok(asr::SpeechEvent::Ready) => { let _ = events.send(DoubaoStatus::Connected).await; }
                    Ok(asr::SpeechEvent::Started) => {
                        if let Some(token) = reply_token.take() { token.cancel(); }
                        playback.interrupt();
                        reply_active = false;
                        let _ = events.send(DoubaoStatus::Connected).await;
                    }
                    Ok(asr::SpeechEvent::Final(transcript)) => {
                        turn = turn.wrapping_add(1);
                        reply_id = reply_id.wrapping_add(1);
                        reply_active = true;
                        let _ = events.send(DoubaoStatus::Thinking).await;
                        let id = reply_id;
                        let token = CancellationToken::new();
                        reply_token = Some(token.clone());
                        let chat = Arc::clone(&chat);
                        let done = done_tx.clone();
                        let status = events.clone();
                        tokio::spawn(async move {
                            let result = chat.respond(transcript, turn, token, status).await;
                            let _ = done.send((id, result)).await;
                        });
                    }
                    Err(_) => break Err("ASR 事件通道已关闭".into()),
                }
            }
            report = reports.recv(), if !reply_active => {
                if let Ok(report) = report {
                    reply_id = reply_id.wrapping_add(1);
                    reply_active = true;
                    let _ = events.send(DoubaoStatus::Speaking).await;
                    let id = reply_id;
                    let token = CancellationToken::new();
                    reply_token = Some(token.clone());
                    let chat = Arc::clone(&chat);
                    let done = done_tx.clone();
                    tokio::spawn(async move {
                        let result = tts::speak(&chat.client, &chat.key, &report, &chat.playback, &token).await;
                        let _ = done.send((id, result)).await;
                    });
                }
            }
            done = done_rx.recv() => {
                if let Ok((id, outcome)) = done {
                    if id == reply_id {
                        reply_active = false;
                        reply_token = None;
                        if let Err(error) = outcome { break Err(error); }
                        let _ = events.send(DoubaoStatus::Connected).await;
                    }
                }
            }
        }
    };
    if let Some(token) = reply_token {
        token.cancel();
    }
    asr_task.abort();
    playback.end();
    result
}
