use super::{publish_reply, DoubaoStatus};
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// 未取消的正常回复应进入历史，先发布文本再发布 Speaking，以继续 TTS。
///
/// 数据构造（含关键数值的推导过程）：历史含一条 user，追加一条 assistant 后总数为 2；
///   AssistantReply 与 Speaking 各占一个槽位，状态通道容量为 2。
/// 执行过程：
///   1. 使用未取消 token 调用生产回复提交函数。
///   2. 检查提交结果、两条具体历史内容和唯一状态更新。
/// 预期结果：允许继续发声；user 和 assistant 均存在；文本先于 Speaking，且不隐式取消 token。
#[tokio::test]
async fn active_reply_is_committed_and_published() {
    let history = Mutex::new(vec![json!({"role":"user","content":"当前输入"})]);
    let token = CancellationToken::new();
    let (status, received) = async_channel::bounded(2);
    assert!(
        publish_reply(&history, "当前回复", &token, &status).await,
        "正常回复应允许继续 TTS"
    );
    let history = history.lock().await;
    assert_eq!(history.len(), 2, "应保留 user 并追加一条 assistant");
    assert_eq!(history[0]["content"], "当前输入", "原用户输入必须保留");
    assert_eq!(
        history[1],
        json!({"role":"assistant","content":"当前回复"}),
        "新 assistant 必须准确提交"
    );
    assert_eq!(
        received.recv().await.expect("应收到文本更新"),
        DoubaoStatus::AssistantReply("当前回复".to_owned()),
        "应先通知 UI 助手回复文本"
    );
    assert_eq!(
        received.recv().await.expect("应收到发声状态"),
        DoubaoStatus::Speaking,
        "应随后通知 UI 正在播报"
    );
    assert!(received.try_recv().is_err(), "不得重复发送状态更新");
    assert!(!token.is_cancelled(), "正常提交不得取消当前回合");
}

/// 历史锁等待期间发生插话后，旧回复不能写入历史或发布 Speaking。
///
/// 数据构造（含关键数值的推导过程）：历史只有一条 user，新生成的旧 assistant 尚未提交；
///   状态通道容量为 1，足以容纳单次 Speaking，用显式 poll 控制锁等待而非时间延迟。
/// 执行过程：
///   1. 持有历史锁，poll 生产回复提交函数，确认它等待该锁。
///   2. 插话取消 token，释放锁并继续执行同一个提交 future。
///   3. 检查历史与状态通道。
/// 预期结果：提交被拒绝，原 user 保留，旧 assistant 不存在，也没有 Speaking。
#[tokio::test]
async fn cancelled_reply_waiting_for_history_is_not_published() {
    let history = Mutex::new(vec![json!({"role":"user","content":"当前输入"})]);
    let guard = history.lock().await;
    let token = CancellationToken::new();
    let (status, received) = async_channel::bounded(1);
    let publish = publish_reply(&history, "旧回复", &token, &status);
    tokio::pin!(publish);
    assert!(
        futures::poll!(&mut publish).is_pending(),
        "旧回复应确实等待历史锁"
    );
    token.cancel();
    drop(guard);
    assert!(!publish.await, "插话取消后不得提交旧回复");
    let history = history.lock().await;
    assert_eq!(history.len(), 1, "历史应只保留已有用户输入");
    assert_eq!(history[0]["content"], "当前输入", "原用户内容必须保留");
    assert!(
        !history.iter().any(|entry| entry["content"] == "旧回复"),
        "取消后的旧 assistant 不得进入历史"
    );
    assert!(
        received.try_recv().is_err(),
        "取消后的旧回复不得发布 Speaking"
    );
}

/// 状态通道拥塞期间发生插话后，旧 Speaking 不能在新 Thinking 之后迟到。
///
/// 数据构造（含关键数值的推导过程）：状态通道容量 = 一个 Thinking，占满后旧 Speaking 等待；
///   已提交的 assistant 写入早于取消，应保留其真实历史，只有迟到状态需要丢弃。
/// 执行过程：
///   1. 填入新回合 Thinking，poll 生产提交函数，确认 Speaking 等待通道空间。
///   2. 取消旧 token，再读取 Thinking 释放空间，继续同一个提交 future。
///   3. 检查是否放弃发声及旧状态是否被丢弃。
/// 预期结果：提交路径停止发声，Thinking 保留正确，队列中没有旧 Speaking。
#[tokio::test]
async fn cancelled_reply_waiting_for_status_does_not_override_thinking() {
    let history = Mutex::new(Vec::new());
    let token = CancellationToken::new();
    let (status, received) = async_channel::bounded(1);
    status
        .try_send(DoubaoStatus::Thinking)
        .expect("Thinking 应占满状态通道");
    let publish = publish_reply(&history, "此前已提交的回复", &token, &status);
    tokio::pin!(publish);
    assert!(
        futures::poll!(&mut publish).is_pending(),
        "旧 Speaking 应确实等待通道空间"
    );
    token.cancel();
    assert_eq!(
        received.recv().await.expect("应读取已有状态"),
        DoubaoStatus::Thinking,
        "新回合 Thinking 必须保持原值"
    );
    assert!(!publish.await, "取消后不得继续旧发声路径");
    assert!(
        received.try_recv().is_err(),
        "释放空间后旧 Speaking 不得迟到"
    );
    assert_eq!(
        history.lock().await[0]["content"],
        "此前已提交的回复",
        "取消前已提交的真实回复不应被删除"
    );
}

/// 场景：文本状态已经排队，但 UI 接收端在 Speaking 入队前关闭。
///
/// 数据构造：容量为 1 的状态通道；AssistantReply 占满后 Speaking 必须等待。
/// 执行过程：启动真实回复提交，读取并验证文本状态，随后同步丢弃接收端。
/// 预期结果：Speaking 发送失败使提交返回 false，调用方不能继续启动 TTS。
#[tokio::test]
async fn closed_status_after_text_prevents_tts() {
    let history = Mutex::new(Vec::new());
    let token = CancellationToken::new();
    let (status, received) = async_channel::bounded(1);
    let publish = publish_reply(&history, "回复", &token, &status);
    tokio::pin!(publish);
    assert!(
        futures::poll!(&mut publish).is_pending(),
        "文本占满通道后，Speaking 应等待接收端"
    );
    assert_eq!(
        received.try_recv().expect("应先收到文本状态"),
        DoubaoStatus::AssistantReply("回复".to_owned()),
        "必须先发布回复文本"
    );
    drop(received);
    assert!(!publish.await, "状态接收端关闭后不得继续 TTS");
}
