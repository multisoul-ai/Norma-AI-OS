use super::support::{report, Conversation, OBSERVE};
use super::{Reply, SpeechEvent};
use crate::app::doubao::{DoubaoReport, DoubaoStatus};
use std::time::{Duration, Instant};
use ui::i18n::LocalizedText;

/// 交给会话后排队过久的通知在播报前被丢弃，不能迟到念出。
///
/// 数据构造：通知一的截止时刻 = now − 1 s（已过期）；通知二无截止时刻。
/// 执行过程：依次排入两条通知，等待控制循环开始播报。
/// 预期结果：只播通知二；通知一的完成回执以失败结束，不能被当成已播。
#[tokio::test]
async fn doubao_expired_report_is_dropped_before_speaking() {
    let mut call = Conversation::start();
    let (mut stale, stale_done) = DoubaoReport::new("过期通知".into());
    stale.expires_at = Some(Instant::now() - Duration::from_secs(1));
    call.reports.send(stale).await.expect("应排入过期通知");
    call.reports
        .send(report("新通知"))
        .await
        .expect("应排入新通知");
    assert_eq!(call.status().await, DoubaoStatus::Speaking, "应播报新通知");
    let spoken = call.reply().await;
    assert!(
        matches!(spoken.reply, Reply::Report(ref text) if text == "新通知"),
        "过期通知不能被播报"
    );
    assert!(stale_done.await.is_err(), "过期通知不能被标记为播完");
    assert!(call.stop().await.is_ok(), "停止应正常释放会话");
    spoken.complete.send(Ok(())).expect("应释放通知请求");
}

/// 旧录音迟到的收音结束不能解除当前录音的说话占用。
///
/// 数据构造：按住录音 id 1、id 2；id 1 迟到结束且带错误；一条任务通知。
///   50 ms 观察窗来自测试调度窗口。
/// 执行过程：按住 1 → 按住 2 → 1 结束 → 排入通知 → 观察 → 2 松键为空。
/// 预期结果：id 2 仍按住时通知不播；id 2 结束后才播。
#[tokio::test]
async fn doubao_stale_listen_end_keeps_current_hold() {
    let mut call = Conversation::start();
    for id in [1, 2] {
        call.speech
            .send(SpeechEvent::HeldStarted(id))
            .await
            .expect("应按住录音");
    }
    call.speech
        .send(SpeechEvent::ListenEnded(
            1,
            Some(LocalizedText::new("doubao.asr_disconnected")),
        ))
        .await
        .expect("应注入旧录音的结束");
    call.reports
        .send(report("任务完成"))
        .await
        .expect("应排入任务通知");
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "当前录音仍按住时通知不能抢播"
    );
    call.speech
        .send(SpeechEvent::HeldReady(2))
        .await
        .expect("应提交当前录音的空结果");
    let spoken = call.reply().await;
    assert!(
        matches!(spoken.reply, Reply::Report(ref text) if text == "任务完成"),
        "当前录音结束后应放行通知"
    );
    assert!(call.stop().await.is_ok(), "停止应正常释放会话");
    spoken.complete.send(Ok(())).expect("应释放通知请求");
}
