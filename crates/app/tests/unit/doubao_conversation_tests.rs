use super::{Reply, SpeechEvent};
use crate::app::doubao::{DoubaoReport, DoubaoStatus};
use ui::i18n::LocalizedText;

use super::support;
use support::{Conversation, OBSERVE};

/// 插话后的旧回复迟到失败不能断开持续聆听。
///
/// 数据构造（含关键数值的推导过程）：两条用户输入、一个旧请求错误；
///   用 50 ms 调度观察窗等待迟到错误被处理，正向等待上限 = 40 × 50 ms = 2 s。
/// 执行过程：
///   1. 第一条 Final 开始回复，随后 Started 取消第一条回复并恢复聆听。
///   2. 旧请求在取消后返回网络错误，等待一个观察窗。
///   3. 第二条 Final 应继续启动新回复，最后显式结束会话。
/// 预期结果：旧 token 已取消；旧失败不能结束会话；新回复存在且未取消。
#[tokio::test]
async fn interrupted_reply_late_error_keeps_listening() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::Final("第一句".into()))
        .await
        .expect("应发送第一句");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "第一句应开始思考"
    );
    let old = call.reply().await;
    call.speech
        .send(SpeechEvent::Started)
        .await
        .expect("应发送插话");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "插话应恢复聆听"
    );
    assert!(old.token.is_cancelled(), "插话必须取消旧回复请求");
    old.complete
        .send(Err(LocalizedText::new("doubao.chat_request")))
        .expect("应注入迟到失败");
    assert!(
        tokio::time::timeout(OBSERVE, &mut call.task).await.is_err(),
        "旧回复迟到错误不得终止持续聆听"
    );
    assert!(!call.cancellation.is_cancelled(), "旧失败不得取消 ASR 会话");
    call.speech
        .send(SpeechEvent::Final("第二句".into()))
        .await
        .expect("应发送第二句");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "第二句仍应开始思考"
    );
    let new = call.reply().await;
    assert!(
        matches!(new.reply, Reply::User { ref transcript, turn: 2 } if transcript == "第二句"),
        "新回合必须接收第二句而非重放旧输入"
    );
    assert!(!new.token.is_cancelled(), "迟到失败不得取消新回复");
    assert!(call.stop().await.is_ok(), "用户结束通话应正常退出");
    new.complete.send(Ok(())).expect("应释放新请求");
}

/// 用户说话期间到达的任务通知必须留队，等待用户回复完成后再播。
///
/// 数据构造（含关键数值的推导过程）：一个 Started、一个通知和一条 Final；
///   50 ms 观察窗确认没有抢播，正向等待上限 = 40 × 50 ms = 2 s。
/// 执行过程：
///   1. Started 进入用户说话阶段，再提交任务通知。
///   2. Final 发起用户回复，用户回复完成之前通知继续排队。
///   3. 用户回复完成后，通知开始播报；开始播放仍没有完成回执。
///   4. 通知播放成功结束 → 才发送完成回执并恢复聆听。
/// 预期结果：通知不抢播；未播完无回执；实际播完才确认消费。
#[tokio::test]
async fn notification_waits_for_user_speech_and_reply() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::Started)
        .await
        .expect("应开始用户语音");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "用户说话时应保持聆听"
    );
    let (notification, mut completed) = DoubaoReport::new("任务完成".into());
    call.reports
        .send(notification)
        .await
        .expect("应提交真实任务通知");
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "用户说话时任务通知不得启动 TTS"
    );
    assert_eq!(call.reports.len(), 1, "未播通知应保留在队列");
    call.speech
        .send(SpeechEvent::Final("请继续".into()))
        .await
        .expect("应结束用户语音");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "应先处理用户输入"
    );
    let user = call.reply().await;
    assert!(
        matches!(user.reply, Reply::User { ref transcript, .. } if transcript == "请继续"),
        "第一个输出必须来自用户输入"
    );
    assert_eq!(call.reports.len(), 1, "用户回复未完成时通知不得出队");
    assert!(call.replies.try_recv().is_err(), "通知不得与用户回复并发");
    user.complete.send(Ok(())).expect("应完成用户回复");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "用户回复完成应恢复聆听"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Speaking,
        "空闲后应开始通知播报"
    );
    let report = call.reply().await;
    assert!(
        matches!(report.reply, Reply::Report(ref text) if text == "任务完成"),
        "通知内容必须保持原始任务结果"
    );
    assert_eq!(call.reports.len(), 0, "已开始的通知不应重复留队");
    assert_eq!(
        completed.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty),
        "只开始播放不能确认通知已消费"
    );
    report.complete.send(Ok(())).expect("应释放通知请求");
    assert!(
        tokio::time::timeout(support::WAIT, completed)
            .await
            .expect("播完后应及时确认")
            .is_ok(),
        "通知实际播完必须发送完成回执"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "播完应恢复聆听"
    );
    assert!(call.stop().await.is_ok(), "用户结束应正常退出");
}

/// Final 直接到达时也必须取消旧通知，迟到成功不能确认已播完或影响新回复。
///
/// 数据构造（含关键数值的推导过程）：一条正在播放的通知和一条新用户 Final；
///   没有 Started，播放代际由初始 0 经一次中断变为 1；观察窗为 50 ms。
/// 执行过程：
///   1. 先启动任务通知，保留旧 TTS 请求与其代际。
///   2. 直接发送用户 Final，启动新回复。
///   3. 旧通知迟到返回成功 → 回执关闭而非确认；新回复随后成功完成。
/// 预期结果：旧通知 token 已取消且无成功回执；新回复未取消，音频代际保持有效。
#[tokio::test]
async fn final_replaces_and_cancels_active_notification() {
    let mut call = Conversation::start();
    let (notification, completed) = DoubaoReport::new("旧通知".into());
    call.reports
        .send(notification)
        .await
        .expect("应提交任务通知");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Speaking,
        "旧通知应开始播报"
    );
    let old = call.reply().await;
    let old_generation = call.playback.generation();
    call.speech
        .send(SpeechEvent::Final("新输入".into()))
        .await
        .expect("应直接发送 Final");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "新输入应开始思考"
    );
    let new = call.reply().await;
    assert!(old.token.is_cancelled(), "替换通知前必须取消旧 TTS 流");
    assert_eq!(
        call.playback.generation(),
        1,
        "直接 Final 必须中断旧播放一次"
    );
    assert_ne!(
        call.playback.generation(),
        old_generation,
        "旧通知音频代际必须失效"
    );
    assert!(
        matches!(new.reply, Reply::User { ref transcript, turn: 1 } if transcript == "新输入"),
        "通知不得占用用户回合编号或覆盖用户内容"
    );
    assert!(!new.token.is_cancelled(), "新用户回合必须保持活跃");
    old.complete.send(Ok(())).expect("应注入取消后的迟到成功");
    assert!(
        tokio::time::timeout(support::WAIT, completed)
            .await
            .expect("取消报告应释放回执")
            .is_err(),
        "已取消通知即使请求成功也不能确认为播完"
    );
    assert!(
        tokio::time::timeout(OBSERVE, &mut call.task).await.is_err(),
        "被替换通知的迟到成功不能结束新回合"
    );
    assert!(
        call.statuses.try_recv().is_err(),
        "旧通知完成不得重置新回合状态"
    );
    assert!(!new.token.is_cancelled(), "旧通知完成不得取消新请求");
    assert_eq!(
        call.playback.generation(),
        1,
        "旧通知迟到完成不得再次中断新音频"
    );
    new.complete.send(Ok(())).expect("应完成新用户回复");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "新回复完成应恢复聆听"
    );
    assert!(
        call.replies.try_recv().is_err(),
        "已被打断的通知不得自动重播"
    );
    assert!(call.stop().await.is_ok(), "用户主动结束应正常退出");
}

/// 删除卡片或关闭通知取消当前报告时，不得把正常取消升级为整通电话失败。
///
/// 数据构造（含关键数值的推导过程）：一条报告及其 App 共享取消 token；
///   初始播放代际 0，取消当前音频后应为 0 + 1 = 1；正向等待上限 2 s = 40 × 50 ms。
/// 执行过程：
///   1. 启动报告，App 侧取消 token，TTS 返回取消错误。
///   2. 报告无成功回执，控制循环应清理音频并回到 Connected。
///   3. 再发送一条用户 Final，验证新请求仍可启动且没有被取消。
/// 预期结果：已取消报告不确认播完；电话继续聆听，后续用户回复正常执行。
#[tokio::test]
async fn externally_cancelled_notification_keeps_call_connected() {
    let mut call = Conversation::start();
    let (notification, completed) = DoubaoReport::new("将删除的卡片通知".into());
    let cancellation = notification.cancellation.clone();
    call.reports.send(notification).await.expect("应提交通知");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Speaking,
        "报告应确实开始播放"
    );
    let report = call.reply().await;
    cancellation.cancel();
    assert!(
        report.token.is_cancelled(),
        "App 取消必须传递给真实报告请求"
    );
    report
        .complete
        .send(Err(LocalizedText::new("doubao.tts_interrupted")))
        .expect("应返回取消错误");
    assert!(
        tokio::time::timeout(support::WAIT, completed)
            .await
            .expect("取消应释放回执")
            .is_err(),
        "取消错误不能发送播完确认"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "取消报告不能断开电话"
    );
    assert_eq!(
        call.playback.generation(),
        1,
        "当前被取消的音频必须中断一次"
    );
    assert!(
        !call.cancellation.is_cancelled(),
        "取消通知不能取消 ASR 会话"
    );
    call.speech
        .send(SpeechEvent::Final("继续对话".into()))
        .await
        .expect("应提交后续输入");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "取消通知后应继续接受用户输入"
    );
    let user = call.reply().await;
    assert!(
        matches!(user.reply, Reply::User { ref transcript, turn: 1 } if transcript == "继续对话"),
        "新回复必须来自后续用户输入"
    );
    assert!(!user.token.is_cancelled(), "后续用户回复不能被旧通知取消");
    user.complete.send(Ok(())).expect("应完成后续回复");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "后续回复完成应恢复聆听"
    );
    assert!(call.stop().await.is_ok(), "结束通话应正常退出");
}

/// 插话后旧回复的迟到成功也不能改变说话状态或放行通知。
///
/// 数据构造（含关键数值的推导过程）：一条被插话取消的用户回复、一个迟到成功结果；
///   50 ms 用于观察完成事件，随后再用 50 ms 观察通知是否错误启动。
/// 执行过程：
///   1. 发起回复后插话，旧服务随后返回成功。
///   2. 检查没有旧完成产生的新状态，再提交任务通知。
///   3. 保持用户说话状态，最后显式结束会话。
/// 预期结果：旧完成不能重置状态；通知仍未播且会话未结束。
#[tokio::test]
async fn interrupted_reply_late_success_does_not_release_notification() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::Final("第一句".into()))
        .await
        .expect("应发送第一句");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "第一句应开始思考"
    );
    let old = call.reply().await;
    call.speech
        .send(SpeechEvent::Started)
        .await
        .expect("应开始插话");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "插话应恢复聆听"
    );
    old.complete.send(Ok(())).expect("应注入迟到成功");
    assert!(
        tokio::time::timeout(OBSERVE, call.statuses.recv())
            .await
            .is_err(),
        "旧回复成功不得发布新的状态更新"
    );
    assert!(old.token.is_cancelled(), "旧服务成功返回不能撤销插话取消");
    call.reports
        .send(super::support::report("仍需排队"))
        .await
        .expect("应提交任务通知");
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "旧完成不得让通知抢占用户说话"
    );
    assert_eq!(call.reports.len(), 1, "用户说话期间通知必须留队");
    assert!(!call.task.is_finished(), "旧成功不得结束通话");
    assert!(call.stop().await.is_ok(), "主动结束应正常退出");
}

/// 当前未取消回复的真实失败必须继续报告，不能被迟到事件隔离逻辑吞掉。
///
/// 数据构造（含关键数值的推导过程）：一条 Final、一个当前 Chat 请求失败；
///   等待上限 = 40 × 50 ms = 2 s，初始播放代际 0 经结束中断变为 1。
/// 执行过程：
///   1. Final 开始当前回复，不发生插话。
///   2. 当前请求返回失败，等待会话完成并检查取消与输出清理。
/// 预期结果：返回明确请求错误，ASR 已取消、播放已结束；不发布成功或自动重试。
#[tokio::test]
async fn current_reply_failure_is_reported_and_cleans_up() {
    let mut call = Conversation::start();
    call.speech
        .send(SpeechEvent::Final("当前请求".into()))
        .await
        .expect("应发送输入");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "当前请求应开始思考"
    );
    let current = call.reply().await;
    current
        .complete
        .send(Err(LocalizedText::new("doubao.chat_request")))
        .expect("应注入当前失败");
    let result = tokio::time::timeout(support::WAIT, &mut call.task)
        .await
        .expect("当前失败必须及时结束会话")
        .expect("控制循环不应 panic");
    assert_eq!(
        result,
        Err(LocalizedText::new("doubao.chat_request")),
        "必须保留当前失败原因"
    );
    assert!(
        call.cancellation.is_cancelled(),
        "当前失败结束时必须取消 ASR"
    );
    assert_eq!(call.playback.generation(), 1, "失败退出必须结束播放队列");
    assert!(
        call.statuses.try_recv().is_err(),
        "当前失败不得发布成功聆听状态"
    );
    assert!(call.replies.try_recv().is_err(), "真实失败不得自动重试请求");
}

/// 同时排队的用户语音和任务通知必须优先处理语音。
///
/// 数据构造（含关键数值的推导过程）：一个通知与一个 Started 在循环运行前同时入队；
///   使用 current-thread runtime 保证同步 try_send 之间无任务切换，观察窗为 50 ms。
/// 执行过程：
///   1. 先同步排入通知，再排入 Started，让两个分支同时可读。
///   2. 启动循环，确认先进入聆听状态且通知留队。
///   3. 主动结束会话。
/// 预期结果：先处理用户语音，不发布 Speaking，不启动通知 TTS。
#[tokio::test]
async fn queued_speech_takes_priority_over_notification() {
    let mut call = Conversation::start();
    call.reports
        .try_send(super::support::report("同时到达的通知"))
        .expect("应同步排入通知");
    call.speech
        .try_send(SpeechEvent::Started)
        .expect("应同步排入语音");
    assert_eq!(
        call.status().await,
        DoubaoStatus::Connected,
        "同时就绪时语音必须优先"
    );
    assert!(
        tokio::time::timeout(OBSERVE, call.replies.recv())
            .await
            .is_err(),
        "通知不得在排队语音之前启动"
    );
    assert_eq!(call.reports.len(), 1, "通知应保留而非被抢先消费");
    assert!(
        call.statuses.try_recv().is_err(),
        "讲话时不得发布通知 Speaking 状态"
    );
    assert!(call.stop().await.is_ok(), "主动结束应正常退出");
}
