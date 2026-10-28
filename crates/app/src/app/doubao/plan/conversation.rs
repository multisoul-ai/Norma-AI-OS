use super::asr::SpeechEvent;
use crate::app::doubao::{playback::PlaybackQueue, DoubaoStatus};
use async_channel::{Receiver, Sender};
use std::{future::Future, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_conversation_backpressure_tests.rs"]
mod backpressure_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/doubao_conversation_freshness_tests.rs"]
mod freshness_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/doubao_hold_conversation_tests.rs"]
mod hold_tests;
#[cfg(test)]
#[path = "../../../../tests/support/doubao_conversation.rs"]
mod support;
#[cfg(test)]
#[path = "../../../../tests/unit/doubao_conversation_tests.rs"]
mod tests;

pub(super) enum Reply {
    User { transcript: String, turn: u64 },
    Report(String),
}

// The reply boundary owns network/tool work; this loop owns listening and output arbitration.
pub(super) async fn run<F, Fut>(
    reply: F,
    playback: Arc<PlaybackQueue>,
    mut asr_task: JoinHandle<Result<(), LocalizedText>>,
    cancellation: CancellationToken,
    events: Sender<DoubaoStatus>,
    speech_rx: Receiver<SpeechEvent>,
    reports: Receiver<super::super::DoubaoReport>,
) -> Result<(), LocalizedText>
where
    F: Fn(Reply, CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), LocalizedText>> + Send + 'static,
{
    let reply = Arc::new(reply);
    let (done_tx, done_rx) = async_channel::unbounded::<(u64, Result<(), LocalizedText>)>();
    let mut reply_token: Option<CancellationToken> = None;
    let mut question_grant: Option<CancellationToken> = None;
    let mut reply_active = false;
    let mut user_speaking = false;
    let mut reports_open = true;
    let mut reply_id = 0_u64;
    let mut turn = 0_u64;
    let mut recording_id = 0_u64;
    let mut held_id: Option<u64> = None;
    let result = loop {
        tokio::select! {
            // Queued speech must win over notifications and stale reply completions.
            biased;
            _ = cancellation.cancelled() => break Ok(()),
            asr_result = &mut asr_task => {
                break asr_result.map_err(|_| LocalizedText::new("doubao.asr_interrupted"))?;
            }
            playback_error = playback.errors.recv() => {
                break Err(playback_error.unwrap_or_else(|_| LocalizedText::new("doubao.output_unavailable")));
            }
            event = speech_rx.recv() => {
                match event {
                    Ok(event @ (SpeechEvent::Ready | SpeechEvent::HeldReady(_))) => {
                        if matches!(event, SpeechEvent::HeldReady(id) if id != recording_id) { continue; }
                        if let SpeechEvent::HeldReady(id) = event {
                            let _ = events.send(DoubaoStatus::RecordingResolved(id)).await;
                        }
                        user_speaking = false;
                        let _ = events.send(DoubaoStatus::Connected).await;
                    }
                    Ok(event @ (SpeechEvent::Started | SpeechEvent::HeldStarted(_))) => {
                        if let SpeechEvent::HeldStarted(id) = event { recording_id = id; held_id = Some(id); }
                        user_speaking = true;
                        if let Some(token) = question_grant.take() { token.cancel(); }
                        if let Some(token) = reply_token.take() { token.cancel(); }
                        // Retire this reply before its cancelled network request can finish.
                        reply_id = reply_id.wrapping_add(1);
                        playback.interrupt();
                        reply_active = false;
                        if matches!(event, SpeechEvent::Started) { let _ = events.send(DoubaoStatus::Connected).await; }
                    }
                    Ok(event @ (SpeechEvent::Final(_) | SpeechEvent::HeldFinal(_, _))) => {
                        let transcript = match event {
                            SpeechEvent::Final(text) => text,
                            SpeechEvent::HeldFinal(id, text) if id == recording_id => text,
                            _ => continue,
                        };
                        if let Some(token) = question_grant.take() { token.cancel(); }
                        // Retire old output before any status send can wait on a full UI queue.
                        if let Some(token) = reply_token.take() {
                            token.cancel();
                            playback.interrupt();
                        }
                        user_speaking = false;
                        if recording_id != 0 {
                            let _ = events.send(DoubaoStatus::RecordingResolved(recording_id)).await;
                        }
                        let _ = events.send(DoubaoStatus::UserTranscript(transcript.clone())).await;
                        turn = turn.wrapping_add(1);
                        reply_id = reply_id.wrapping_add(1);
                        reply_active = true;
                        let _ = events.send(DoubaoStatus::Thinking).await;
                        let id = reply_id;
                        let token = cancellation.child_token();
                        if crate::app::MultisoulApp::is_question_delegation(&transcript) {
                            question_grant = Some(token.clone());
                        }
                        reply_token = Some(token.clone());
                        let reply = Arc::clone(&reply);
                        let done = done_tx.clone();
                        tokio::spawn(async move {
                            let result = reply(Reply::User { transcript, turn }, token).await;
                            let _ = done.send((id, result)).await;
                        });
                    }
                    Ok(SpeechEvent::ListenEnded(id, error)) => {
                        // 收音结束（含失败）只解除说话占用，通道继续播报；按住模式忽略旧录音的迟到结束。
                        if held_id.is_none_or(|current| current == id) { user_speaking = false; }
                        let _ = events.send(DoubaoStatus::ListenEnded(id, error)).await;
                    }
                    Err(_) => break (&mut asr_task).await.map_err(|_| LocalizedText::new("doubao.asr_interrupted"))?,
                }
            }
            report = reports.recv(), if reports_open && !user_speaking && !reply_active => {
                if let Ok(report) = report {
                    // 交给会话后仍可能等用户说完；过期的通知直接丢弃，不迟到播报。
                    if report.cancellation.is_cancelled()
                        || report.expires_at.is_some_and(|expires_at| std::time::Instant::now() >= expires_at) {
                        if let Some(delivery) = &report.delivery { delivery.cancel(); }
                        continue;
                    }
                    reply_id = reply_id.wrapping_add(1);
                    reply_active = true;
                    let _ = events.send(DoubaoStatus::Speaking).await;
                    let id = reply_id;
                    let delivery = report.delivery.clone();
                    if let Some(delivery) = &delivery {
                        if let Err(error) = playback.track_report(playback.generation(), delivery.clone()) {
                            delivery.set(agent::context::ReportDelivery::Failed);
                            break Err(error);
                        }
                    }
                    let token = report.cancellation;
                    reply_token = Some(token.clone());
                    let reply = Arc::clone(&reply);
                    let done = done_tx.clone();
                    tokio::spawn(async move {
                        let result = reply(Reply::Report(report.text), token.clone()).await;
                        if let Some(delivery) = &delivery {
                            // Cancellation is classified by the audio worker after hardware stop.
                            if result.is_err() && !token.is_cancelled() { delivery.set(agent::context::ReportDelivery::Failed); }
                        }
                        if result.is_ok() && !token.is_cancelled() {
                            let _ = report.completed.send(());
                        }
                        let _ = done.send((id, result)).await;
                    });
                } else {
                    // A closed optional notification source must not spin the select loop.
                    reports_open = false;
                }
            }
            done = done_rx.recv() => {
                if let Ok((id, outcome)) = done {
                    if id == reply_id {
                        reply_active = false;
                        // This loop owns interruption: a retired reply cannot stop newer audio.
                        if reply_token.take().is_some_and(|token| token.is_cancelled()) {
                            playback.interrupt();
                        } else if let Err(error) = outcome { break Err(error); }
                        let _ = events.send(DoubaoStatus::Connected).await;
                    }
                }
            }
        }
    };
    if let Some(token) = reply_token {
        token.cancel();
    }
    cancellation.cancel();
    if !asr_task.is_finished() {
        let _ = asr_task.await;
    }
    playback.end();
    result
}
