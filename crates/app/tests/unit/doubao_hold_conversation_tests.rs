use super::support::{Conversation, OBSERVE};
use super::{Reply, SpeechEvent};
use crate::app::doubao::DoubaoStatus;

/// 新按住动作立即打断旧回复；旧录音的迟到转写不得抢占新录音。
///
/// 数据构造（含关键数值的推导过程）：录音 id 1 和 2；初始回复为普通 Final；
///   50 ms 观察窗来自测试调度窗口，正向等待上限 = 40 × 50 ms = 2 s。
/// 执行过程：开始旧回复 → 按住 id 1 → 再按住 id 2 → 返回 id 1 迟到结果
///   → id 2 松键结果发起新回复 → 显式停止会话。
/// 预期结果：旧 token 取消；迟到转写不发起回复；仅“新输入”进入第二个用户回合。
#[tokio::test]
async fn doubao_hold_start_interrupts_and_rejects_stale_transcripts() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::Final("旧回复".into()))
        .await
        .expect("应开始旧回复");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "旧输入应进入思考"
    );
    let old = call.reply().await;
    call.speech
        .send(SpeechEvent::HeldStarted(1))
        .await
        .expect("应按住录音一");
    call.speech
        .send(SpeechEvent::HeldStarted(2))
        .await
        .expect("应按住录音二");
    call.speech
        .send(SpeechEvent::HeldFinal(1, "过期输入".into()))
        .await
        .expect("应注入迟到转写");
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "旧录音的转写不能在新录音时发给 AI"
    );
    assert!(
        old.token.is_cancelled(),
        "按住动作不能等网络识别完成才取消旧回复"
    );
    assert!(
        call.statuses.try_recv().is_err(),
        "迟到转写不能覆盖前台的录音状态"
    );
    call.speech
        .send(SpeechEvent::HeldFinal(2, "新输入".into()))
        .await
        .expect("应提交当前录音");
    assert_eq!(
        call.status().await,
        DoubaoStatus::RecordingResolved(2),
        "当前转写必须携带录音 id，让 UI 解除旧状态隔离"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "当前录音松键后应进入思考"
    );
    let current = call.reply().await;
    assert!(
        matches!(current.reply, Reply::User { ref transcript, turn: 2 } if transcript == "新输入"),
        "只允许新输入进入第二回合"
    );
    assert!(!current.token.is_cancelled(), "旧录音不能取消当前回复");
    assert!(call.stop().await.is_ok(), "停止应释放识别与播放");
    old.complete.send(Ok(())).expect("应释放旧回复");
    current.complete.send(Ok(())).expect("应释放当前回复");
}

/// 空录音松开后恢复空闲，并允许之前排队的任务回执播报。
///
/// 数据构造（含关键数值的推导过程）：一个 HeldStarted、一个 HeldReady 和一个任务通知；
///   50 ms 观察窗验证按住期间通知没有抢播，正向等待上限 = 40 × 50 ms = 2 s。
/// 执行过程：按住但不说话 → 排入通知 → 松键识别为空 → 播报排队通知 → 停止。
/// 预期结果：按住期间不抢播；松键后没有空 AI 回合且通知可以出队。
#[tokio::test]
async fn doubao_hold_empty_recording_releases_queued_reports() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::HeldStarted(1))
        .await
        .expect("应开始录音");
    call.reports
        .send(super::support::report("任务完成"))
        .await
        .expect("应排入任务通知");
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "按住时通知不能抢播"
    );
    call.speech
        .send(SpeechEvent::HeldReady(1))
        .await
        .expect("应提交空识别终态");
    assert_eq!(
        call.status().await,
        DoubaoStatus::RecordingResolved(1),
        "空录音也必须解除对应录音的状态隔离"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "空录音应恢复空闲"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Speaking,
        "松键后应放行排队通知"
    );
    let report = call.reply().await;
    assert!(
        matches!(report.reply, Reply::Report(ref text) if text == "任务完成"),
        "空录音不应产生用户 AI 回合"
    );
    assert_eq!(call.reports.len(), 0, "通知应从队列移除且不会重复播放");
    assert!(
        !call.cancellation.is_cancelled(),
        "空录音不能结束整段对话会话"
    );
    assert!(call.stop().await.is_ok(), "停止应正常释放会话");
    report.complete.send(Ok(())).expect("应释放通知请求");
}
