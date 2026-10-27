use super::*;
use std::{
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
};

struct WaitingSink {
    ready: Arc<AtomicBool>,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    messages: Arc<Mutex<Vec<String>>>,
    close: CancellationToken,
}
impl Sink<Message> for WaitingSink {
    type Error = tokio_tungstenite::tungstenite::Error;
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.ready.load(Ordering::Acquire) {
            return Poll::Ready(Ok(()));
        }
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
    fn start_send(self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        let text = message.to_text()?.to_owned();
        if text == "barrier" {
            self.close.cancel();
        }
        self.messages.lock().unwrap().push(text);
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

/// 场景：报告已出队，但 socket 尚未 ready；此时取消仍应阻止开始写入。
/// 构造：可阻塞 poll_ready 的真实 transmit Sink，后接普通屏障命令和关闭令牌。
/// 执行：等发送器实际进入 Pending 后取消报告，再释放 Sink，等待正常关闭。
/// 预期：start_send 只观察到屏障和 close；未开始写的报告不会在恢复后发送。
#[tokio::test]
async fn cancellation_while_socket_not_ready_prevents_report_start_send() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let ready = Arc::new(AtomicBool::new(false));
        let messages = Arc::new(Mutex::new(Vec::new()));
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let close = CancellationToken::new();
        let scope = CancellationToken::new();
        let sink = WaitingSink {
            ready: ready.clone(),
            entered: Some(entered),
            messages: messages.clone(),
            close: close.clone(),
        };
        let mut session = LiveSession::new(LiveConfig::default());
        session.start_message().unwrap();
        session
            .receive(
                r#"{"type":"session.started","session":{"id":"waiting","model":"gpt-live-1"}}"#,
            )
            .unwrap();
        let (_audio, input) = mpsc::channel(1);
        let (commands, queued) = mpsc::channel(2);
        commands
            .try_send(CommandFrame {
                message: "old-report".into(),
                scope: Some(scope.clone()),
            })
            .unwrap();
        commands
            .try_send(CommandFrame {
                message: "barrier".into(),
                scope: None,
            })
            .unwrap();
        let worker = tokio::spawn(transmit(
            sink,
            Arc::new(Mutex::new(session)),
            input,
            queued,
            Arc::new(Mutex::new(InputGate::default())),
            close,
        ));
        waiting.await.unwrap();
        scope.cancel();
        ready.store(true, Ordering::Release);
        assert!(worker.await.unwrap().is_ok(), "局部取消后仍应完成正常关闭");
        let messages = messages.lock().unwrap();
        assert_eq!(messages.len(), 2, "只有屏障和关闭，不含旧报告");
        assert_eq!(messages[0], "barrier", "未 ready 的报告不能调用 start_send");
        assert!(
            messages[1].contains("session.close"),
            "最后必须真正关闭会话"
        );
    })
    .await
    .expect("慢写入取消测试必须有界");
}
