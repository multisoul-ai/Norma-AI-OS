use super::{Reply, SpeechEvent};
use crate::app::doubao::DoubaoReport;
use crate::app::doubao::{playback::start_playback_with, DoubaoStatus};
use crate::app::voice::playback::RecordingAssistantAudioSink;
use crate::app::voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus};
use async_channel::{Receiver, Sender};
use std::{sync::Arc, time::Duration};
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

// One scheduler observation window is 50 ms; positive waits allow 40 windows = 2 s.
pub(super) const OBSERVE: Duration = Duration::from_millis(50);
pub(super) const WAIT: Duration = Duration::from_secs(2);

pub(super) struct PendingReply {
    pub reply: Reply,
    pub token: CancellationToken,
    pub complete: oneshot::Sender<Result<(), LocalizedText>>,
}

pub(super) struct Conversation {
    pub speech: Sender<SpeechEvent>,
    pub reports: Sender<DoubaoReport>,
    pub statuses: Receiver<DoubaoStatus>,
    pub replies: Receiver<PendingReply>,
    pub playback: Arc<crate::app::doubao::playback::PlaybackQueue>,
    pub cancellation: CancellationToken,
    pub task: JoinHandle<Result<(), LocalizedText>>,
}

impl Conversation {
    pub fn start() -> Self {
        let cancellation = CancellationToken::new();
        let asr_cancel = cancellation.clone();
        let asr_task = tokio::spawn(async move {
            asr_cancel.cancelled().await;
            Ok(())
        });
        let (bus, _wake) = VoiceWindowInsetEventBus::new();
        let playback = Arc::new(start_playback_with(
            || Box::new(RecordingAssistantAudioSink::new()),
            bus.sink(TtsPlaybackSource::Doubao),
        ));
        let (speech, speech_rx) = async_channel::unbounded();
        let (reports, report_rx) = async_channel::unbounded();
        let (status_tx, statuses) = async_channel::unbounded();
        let (reply_tx, replies) = async_channel::unbounded();
        let task = tokio::spawn(super::run(
            move |reply, token| {
                let replies = reply_tx.clone();
                async move {
                    let (complete, completed) = oneshot::channel();
                    replies
                        .send(PendingReply {
                            reply,
                            token,
                            complete,
                        })
                        .await
                        .expect("测试应接收真实控制循环发起的回复");
                    completed.await.expect("测试应明确控制服务完成结果")
                }
            },
            Arc::clone(&playback),
            asr_task,
            cancellation.clone(),
            status_tx,
            speech_rx,
            report_rx,
        ));
        Self {
            speech,
            reports,
            statuses,
            replies,
            playback,
            cancellation,
            task,
        }
    }

    pub async fn status(&self) -> DoubaoStatus {
        tokio::time::timeout(WAIT, self.statuses.recv())
            .await
            .expect("控制循环应及时更新通话状态")
            .expect("通话不应在预期状态之前结束")
    }

    pub async fn reply(&self) -> PendingReply {
        tokio::time::timeout(WAIT, self.replies.recv())
            .await
            .expect("应及时开始预期回复")
            .expect("回复通道不应提前关闭")
    }

    pub async fn stop(&mut self) -> Result<(), LocalizedText> {
        self.cancellation.cancel();
        tokio::time::timeout(WAIT, &mut self.task)
            .await
            .expect("取消后控制循环应及时结束")
            .expect("控制循环不应 panic")
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub(super) fn report(text: &str) -> DoubaoReport {
    DoubaoReport::new(text.into()).0
}
