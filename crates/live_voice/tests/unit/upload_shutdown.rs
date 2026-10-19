use super::*;
use futures::Sink;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio_tungstenite::tungstenite;

struct ClosingSink {
    cancellation: CancellationToken,
    messages: Arc<Mutex<Vec<String>>>,
    audio_count: usize,
}
impl Sink<Message> for ClosingSink {
    type Error = tungstenite::Error;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        let text = item.to_text()?.to_owned();
        if text.contains("session.input_audio.append") {
            self.audio_count += 1;
        }
        self.messages.lock().expect("发送记录").push(text);
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The 65th successful send overlaps close while its 64 prior observations fill the queue.
        if self.audio_count == 65 {
            self.cancellation.cancel();
        }
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

/// AC03/AC11：发送成功后的观察背压不能抢走同时到来的正常关闭。
/// 数据构造：65个10ms包共20800B，小于32000B预算；观察队列64项，第65次flush触发close。
/// 执行过程：通过真实transmit和可控Sink推进节奏、填满观察队列，再在send.await中请求挂断。
/// 预期结果：发送器返回Ok并确实发出session.close，不能因观察队列Full提前返回Capacity。
#[tokio::test(start_paused = true)]
async fn closing_during_send_overrides_full_upload_observer_queue() {
    let cancellation = CancellationToken::new();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = ClosingSink {
        cancellation: cancellation.clone(),
        messages: messages.clone(),
        audio_count: 0,
    };
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("start");
    session.receive(r#"{"type":"session.started","session":{"id":"live_close_upload","model":"gpt-live-1"}}"#).expect("ready");
    let (audio_tx, audio_rx) = mpsc::channel(65);
    let (_commands_tx, commands_rx) = mpsc::channel(1);
    let (observer, _observed) = mpsc::channel(64);
    let gate = Arc::new(Mutex::new(InputGate {
        observer: Some(observer),
        ..Default::default()
    }));
    let budget = Arc::new(Semaphore::new(AUDIO_QUEUE_BYTES));
    for _ in 0..65 {
        let pcm =
            PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16000, 1, vec![0; 320]).expect("10ms PCM");
        audio_tx
            .try_send(AudioFrame {
                pcm,
                generation: 0,
                synthetic_silence: false,
                _permit: budget
                    .clone()
                    .try_acquire_many_owned(320)
                    .expect("小于1秒预算"),
            })
            .expect("预置65包");
    }
    let result = transmit(
        sink,
        Arc::new(Mutex::new(session)),
        audio_rx,
        commands_rx,
        gate,
        cancellation,
    )
    .await;
    assert!(result.is_ok(), "正常close优先于观察背压：{result:?}");
    let messages = messages.lock().expect("发送记录");
    assert!(
        messages
            .last()
            .is_some_and(|text| text.contains("session.close")),
        "发送器必须真正发送close再结束"
    );
    assert_eq!(messages.len(), 66, "65个append加1个close");
}
