use super::support::{self, Conversation};
use super::{Reply, SpeechEvent};
use crate::app::doubao::DoubaoStatus;

/// 场景：状态队列满时直接收到 Final，旧播放必须先被打断。
///
/// 数据构造：容量为 1 的状态队列由旧报告的 Speaking 占满；旧报告请求尚未完成。
/// 执行过程：直接发送新 Final，不先发送 Started，也不读取队列；等待旧 token 取消，
///   再依次读取旧 Speaking、新转写和 Thinking，并释放新请求。
/// 预期结果：取消不依赖 UI 消费速度；旧播放代际失效，新回合仍能继续。
#[tokio::test]
async fn final_interrupts_old_playback_before_blocked_transcript_status() {
    let mut call = Conversation::start_with_status_capacity(Some(1));
    call.reports
        .send(support::report("旧报告"))
        .await
        .expect("应提交旧报告");
    let old = call.reply().await;
    let generation = call.playback.generation();
    call.speech
        .send(SpeechEvent::Final("新输入".into()))
        .await
        .expect("应直接提交新输入");
    assert!(
        tokio::time::timeout(support::WAIT, old.token.cancelled())
            .await
            .is_ok(),
        "即使状态队列已满，旧报告也必须立即取消"
    );
    assert_ne!(
        call.playback.generation(),
        generation,
        "旧播放必须在等待 UI 消费转写前失效"
    );
    assert_eq!(
        call.status().await,
        DoubaoStatus::Speaking,
        "旧报告状态原本已在队列中"
    );
    call.expect_user_transcript("新输入").await;
    assert_eq!(
        call.status().await,
        DoubaoStatus::Thinking,
        "新转写之后应进入新回合"
    );
    let new = call.reply().await;
    assert!(
        matches!(new.reply, Reply::User { ref transcript, turn: 1 } if transcript == "新输入"),
        "新回合应使用直接到达的 Final"
    );
    assert!(call.stop().await.is_ok(), "用户停止应释放控制循环");
    old.complete.send(Ok(())).expect("应释放旧请求");
    new.complete.send(Ok(())).expect("应释放新请求");
}
