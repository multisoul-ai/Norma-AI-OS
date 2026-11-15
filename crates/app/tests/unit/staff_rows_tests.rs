use super::*;
use norma::protocol::{ReplyStatus, ReplyTrigger, WorkerOutcome};

fn started() -> TaskMessage {
    TaskMessage::WorkerStarted {
        proposal_id: 1,
        dispatch_id: "d".into(),
    }
}

/// 场景：进行中的回复固定在底部，它运行期间到达的事件排在它上方；Answered 不单独成行。
/// 数据构造：[用户, 已完成回复, Answered, 进行中回复, Worker 启动事件, Worker 结果]（下标 0..=5）。
/// 执行过程：调用 rows 取显示顺序。
/// 预期结果：行顺序为 0,1,4,5,3——Answered(2) 被跳过，进行中回复(3) 排在最后。
#[test]
fn running_reply_is_last_and_answered_is_hidden() {
    let mut task = Task::new("t", "w", "p", "x");
    task.user_message("问");
    task.begin_reply(ReplyTrigger::User)
        .finish(ReplyStatus::Done, 1);
    task.answered_message("q");
    task.begin_reply(ReplyTrigger::User);
    task.messages.push(started());
    task.messages.push(TaskMessage::WorkerFinished {
        node_id: 1,
        dispatch_id: "d".into(),
        outcome: WorkerOutcome::Succeeded,
        text: "ok".into(),
    });
    assert_eq!(
        rows(&task),
        vec![0, 1, 4, 5, 3],
        "运行中的回复在最底部，Answered 被隐藏"
    );
}

/// 场景：没有进行中的回复时保持时间顺序。
/// 数据构造：[用户, 已完成回复, 事件]。
/// 执行过程：调用 rows 取显示顺序。
/// 预期结果：行顺序 0,1,2，不重排。
#[test]
fn without_a_running_reply_rows_keep_time_order() {
    let mut task = Task::new("t", "w", "p", "x");
    task.user_message("问");
    task.begin_reply(ReplyTrigger::User)
        .finish(ReplyStatus::Done, 1);
    task.messages.push(started());
    assert_eq!(rows(&task), vec![0, 1, 2], "按时间顺序");
}
