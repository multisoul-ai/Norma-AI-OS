//! 收音监听循环：一次收音失败只结束该次收音，通道会话继续等待下一次 Cmd+E。
use super::{run_at, SpeechEvent};
use crate::app::doubao::input::{ListenRequest, RecordingInput};
use async_channel::{Receiver, Sender};
use std::future::Future;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

#[cfg(test)]
#[path = "../../../../../tests/unit/doubao_listen_tests.rs"]
mod tests;

fn asr_url() -> String {
    super::plan_url(
        "NORMA_DOUBAO_ASR_URL",
        "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async",
    )
}

pub(in crate::app::doubao) async fn listen_continuous(
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    listens: Receiver<ListenRequest>,
) -> Result<(), LocalizedText> {
    let url = asr_url();
    let runner_events = events.clone();
    listen_continuous_with(
        move |token| run_at(url.clone(), key.clone(), runner_events.clone(), token, None),
        events,
        cancellation,
        listens,
    )
    .await
}

pub(super) async fn listen_continuous_with<F, Fut>(
    run: F,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    listens: Receiver<ListenRequest>,
) -> Result<(), LocalizedText>
where
    F: Fn(CancellationToken) -> Fut,
    Fut: Future<Output = Result<(), LocalizedText>>,
{
    loop {
        let (id, stop) = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(()),
            listen = listens.recv() => match listen { Ok(listen) => listen, Err(_) => return Ok(()) },
        };
        if stop.is_cancelled() {
            continue;
        }
        // 收音同时受会话与本次 Cmd+E 令牌控制，任一取消都释放麦克风。
        let token = cancellation.child_token();
        let request = run(token.clone());
        tokio::pin!(request);
        let result = tokio::select! {
            biased;
            _ = stop.cancelled() => {
                token.cancel();
                request.await
            }
            result = &mut request => result,
        };
        token.cancel();
        if cancellation.is_cancelled() {
            return Ok(());
        }
        if let Err(error) = &result {
            tracing::warn!(target: "norma_voice", listen_id = id, reason = %error,
                "Doubao continuous listening ended with error");
        }
        let _ = events
            .send(SpeechEvent::ListenEnded(id, result.err()))
            .await;
    }
}

pub(in crate::app::doubao) async fn listen_recordings(
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    recordings: Receiver<RecordingInput>,
) -> Result<(), LocalizedText> {
    listen_recordings_at(asr_url(), key, events, cancellation, recordings).await
}

pub(super) async fn listen_recordings_at(
    url: String,
    key: String,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    recordings: Receiver<RecordingInput>,
) -> Result<(), LocalizedText> {
    let runner_events = events.clone();
    listen_recordings_with(
        move |token, input| {
            run_at(
                url.clone(),
                key.clone(),
                runner_events.clone(),
                token,
                Some(input),
            )
        },
        events,
        cancellation,
        recordings,
    )
    .await
}

pub(super) async fn listen_recordings_with<F, Fut>(
    run: F,
    events: Sender<SpeechEvent>,
    cancellation: CancellationToken,
    recordings: Receiver<RecordingInput>,
) -> Result<(), LocalizedText>
where
    F: Fn(CancellationToken, RecordingInput) -> Fut,
    Fut: Future<Output = Result<(), LocalizedText>>,
{
    let mut next = None;
    loop {
        let input = if let Some(input) = next.take() {
            input
        } else {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                input = recordings.recv() => match input { Ok(input) => input, Err(_) => return Ok(()) },
            }
        };
        let id = input.0;
        let token = cancellation.child_token();
        let request = run(token.clone(), input);
        tokio::pin!(request);
        next = tokio::select! {
            // 新输入优先于旧请求完成，旧连接错误不能终止已开始的新录音。
            biased;
            _ = cancellation.cancelled() => {
                token.cancel();
                let _ = request.await;
                return Ok(());
            },
            input = recordings.recv() => {
                token.cancel();
                let _ = request.await;
                match input { Ok(input) => Some(input), Err(_) => return Ok(()) }
            },
            result = &mut request => {
                if let Err(error) = result {
                    tracing::warn!(target: "norma_voice", recording_id = id, reason = %error,
                        "Doubao held recording ended with error");
                    let _ = events.send(SpeechEvent::ListenEnded(id, Some(error))).await;
                }
                None
            },
        };
    }
}
