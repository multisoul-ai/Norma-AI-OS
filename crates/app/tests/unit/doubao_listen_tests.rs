use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

// 正向等待上限 2 s = 40 × 50 ms 调度窗口。
const WAIT: Duration = Duration::from_secs(2);

async fn next_event(events: &async_channel::Receiver<SpeechEvent>) -> SpeechEvent {
    tokio::time::timeout(WAIT, events.recv())
        .await
        .expect("应及时收到收音事件")
        .expect("事件通道不应关闭")
}

/// 持续聆听：一次收音失败只结束该次收音，监听继续等待下一次 Cmd+E。
///
/// 数据构造：第 1 次收音（id 7）返回 asr_disconnected；第 2 次收音（id 8）等待令牌取消后返回 Ok。
/// 执行过程：下发 id 7 → 收到失败结束事件 → 下发 id 8 → 取消 id 8 令牌 → 取消会话。
/// 预期结果：id 7 产生 ListenEnded(7, Some(错误))；监听任务没有结束且启动了第 2 次收音；
///   id 8 由用户主动结束，产生 ListenEnded(8, None)；会话取消后任务返回 Ok。
#[tokio::test]
async fn doubao_continuous_listen_failure_keeps_listener_alive() {
    let runs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&runs);
    let (events_tx, events_rx) = async_channel::unbounded();
    let (listens_tx, listens_rx) = async_channel::unbounded();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(listen_continuous_with(
        move |token: CancellationToken| {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 0 {
                    return Err(LocalizedText::new("doubao.asr_disconnected"));
                }
                token.cancelled().await;
                Ok(())
            }
        },
        events_tx,
        cancellation.clone(),
        listens_rx,
    ));
    listens_tx
        .send((7, CancellationToken::new()))
        .await
        .expect("应下发第 1 次收音");
    assert!(
        matches!(
            next_event(&events_rx).await,
            SpeechEvent::ListenEnded(7, Some(ref error))
                if *error == LocalizedText::new("doubao.asr_disconnected")
        ),
        "收音失败应带 id 和原因上报"
    );
    assert!(!task.is_finished(), "收音失败不能结束监听任务");
    let second = CancellationToken::new();
    listens_tx
        .send((8, second.clone()))
        .await
        .expect("应下发第 2 次收音");
    tokio::time::timeout(WAIT, async {
        while runs.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("失败后应能开始下一次收音");
    second.cancel();
    assert!(
        matches!(
            next_event(&events_rx).await,
            SpeechEvent::ListenEnded(8, None)
        ),
        "用户停止收音应以无错误结束"
    );
    cancellation.cancel();
    assert!(
        tokio::time::timeout(WAIT, task)
            .await
            .expect("取消后应及时退出")
            .expect("监听任务不应 panic")
            .is_ok(),
        "会话取消应正常结束监听"
    );
}

/// 会话取消会终止正在进行的收音，且不再上报收音结束。
///
/// 数据构造：收音 id 1 的执行体一直等到自身令牌被取消。
/// 执行过程：下发 id 1 → 取消会话。
/// 预期结果：收音令牌随会话取消；任务返回 Ok；不产生 ListenEnded 事件。
#[tokio::test]
async fn doubao_continuous_listen_stops_with_session() {
    let (events_tx, events_rx) = async_channel::unbounded();
    let (listens_tx, listens_rx) = async_channel::unbounded();
    let cancellation = CancellationToken::new();
    let (started_tx, started_rx) = async_channel::bounded(1);
    let task = tokio::spawn(listen_continuous_with(
        move |token: CancellationToken| {
            let started = started_tx.clone();
            async move {
                let _ = started.send(()).await;
                token.cancelled().await;
                Ok(())
            }
        },
        events_tx,
        cancellation.clone(),
        listens_rx,
    ));
    listens_tx
        .send((1, CancellationToken::new()))
        .await
        .expect("应下发收音");
    tokio::time::timeout(WAIT, started_rx.recv())
        .await
        .expect("收音应及时开始")
        .expect("开始信号不应丢失");
    cancellation.cancel();
    assert!(
        tokio::time::timeout(WAIT, task)
            .await
            .expect("会话取消应终止收音")
            .expect("监听任务不应 panic")
            .is_ok(),
        "会话取消应正常结束"
    );
    assert!(
        events_rx.try_recv().is_err(),
        "会话关闭时不应再上报收音结束"
    );
}

/// 按住说话：一次录音的识别失败只结束该次录音，会话继续接受下一次录音。
///
/// 数据构造：录音 id 4 的识别返回 asr_connect_timeout；录音 id 5 识别成功。
/// 执行过程：提交 id 4 → 收到失败事件 → 提交 id 5 → 取消会话。
/// 预期结果：ListenEnded(4, Some(超时))；任务未结束；第 2 次识别被执行；取消后 Ok。
#[tokio::test]
async fn doubao_held_listen_failure_keeps_session_alive() {
    let runs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&runs);
    let (events_tx, events_rx) = async_channel::unbounded();
    let (recordings_tx, recordings_rx) = async_channel::unbounded();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(listen_recordings_with(
        move |_token: CancellationToken,
              (id, _input): crate::app::doubao::input::RecordingInput| {
            counter.fetch_add(1, Ordering::SeqCst);
            async move {
                if id == 4 {
                    Err(LocalizedText::new("doubao.asr_connect_timeout"))
                } else {
                    Ok(())
                }
            }
        },
        events_tx,
        cancellation.clone(),
        recordings_rx,
    ));
    let (_first_tx, first_rx) = async_channel::unbounded();
    recordings_tx
        .send((4, first_rx))
        .await
        .expect("应提交录音 4");
    assert!(
        matches!(
            next_event(&events_rx).await,
            SpeechEvent::ListenEnded(4, Some(ref error))
                if *error == LocalizedText::new("doubao.asr_connect_timeout")
        ),
        "识别失败应带录音 id 上报"
    );
    assert!(!task.is_finished(), "识别失败不能结束会话");
    let (_second_tx, second_rx) = async_channel::unbounded();
    recordings_tx
        .send((5, second_rx))
        .await
        .expect("应提交录音 5");
    tokio::time::timeout(WAIT, async {
        while runs.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("失败后应能开始下一次录音识别");
    cancellation.cancel();
    assert!(
        tokio::time::timeout(WAIT, task)
            .await
            .expect("取消后应及时退出")
            .expect("监听任务不应 panic")
            .is_ok(),
        "会话取消应正常结束"
    );
}
