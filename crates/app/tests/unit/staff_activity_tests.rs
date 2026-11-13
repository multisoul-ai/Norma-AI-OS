use super::*;
use brain::acp::trace::TraceItem;

/// 场景：先收到新轮开始、后加载旧历史，旧Norma消息不能吞掉新分片。
/// 数据构造：旧历史1条用户+1条Norma=2条，新用户1条；Brain基线为3，UI尚未加载。
/// 执行过程：使用Brain基线开始 → 异步历史加载 → 本轮首分片到达。
/// 预期结果：首分片仍可见，旧回复不被当作本轮落盘，等待状态保持。
#[test]
fn late_history_does_not_commit_the_new_stream() {
    let mut task = Task::new("t", "w", "p", "title");
    task.user_message("旧问题");
    task.messages.push(TaskMessage::Norma {
        text: "旧回复".into(),
    });
    task.user_message("新问题");
    let mut state = TaskActivity::default();
    state.apply(1, TaskUpdate::Started { message_count: 3 });
    state.reconcile(&task);
    state.apply(
        1,
        TaskUpdate::Update(TraceItem::new(TraceKind::Reply, "", "新分片")),
    );
    assert_eq!(state.reply, "新分片", "迟到的旧历史不能丢弃本轮流式输出");
    assert!(!state.committed, "旧回复不属于当前轮次");
    assert!(state.waiting(), "未结束的新轮仍须显示响应状态");
}

/// 场景：任务排队、流式、计划替换、持久化交接与迟到事件保持正确。
/// 数据构造：旧轮1、新轮2；计划先pending后completed；持久化回复“完整”。
/// 执行过程：排队→开始2→半段/计划→保存完整→迟到半段→旧轮结束→当前结束。
/// 预期结果：排队立即可见；plan替换不重复；保存后无双气泡；旧轮不能结束新轮。
#[test]
fn progress_reconciles_saved_reply_and_rejects_old_turn() {
    let mut state = TaskActivity::default();
    let mut task = Task::new("t", "w", "p", "title");
    state.apply(0, TaskUpdate::Queued);
    assert!(state.waiting(), "入队即反馈，不能等首token");
    state.apply(2, TaskUpdate::Started { message_count: 0 });
    state.apply(
        2,
        TaskUpdate::Update(TraceItem::new(TraceKind::Reply, "", "半段")),
    );
    assert_eq!(state.reply, "半段", "应立即积累流式文字");
    state.apply(
        2,
        TaskUpdate::Update(TraceItem::new(TraceKind::Plan, "", "[pending] 检查")),
    );
    state.apply(
        2,
        TaskUpdate::Update(TraceItem::new(TraceKind::Plan, "", "[completed] 检查")),
    );
    assert_eq!(
        state.plan,
        vec![("completed".into(), "检查".into())],
        "计划更新须替换旧状态"
    );
    task.messages.push(TaskMessage::Norma {
        text: "完整".into(),
    });
    state.reconcile(&task);
    state.apply(
        2,
        TaskUpdate::Update(TraceItem::new(TraceKind::Reply, "", "完整")),
    );
    assert!(
        state.reply.is_empty(),
        "已落盘的CLI回复不能又画一份流式气泡"
    );
    state.apply(1, TaskUpdate::Finished { failed: true });
    assert!(state.active, "迟到旧轮结束不能清掉当前动画");
    assert!(!state.failed, "旧轮失败不能污染新轮");
    state.apply(2, TaskUpdate::Finished { failed: false });
    assert!(!state.waiting(), "完成后必须停止等待动画");
}

/// 场景：服务停止时排队和进行中任务都停止动画，但保留失败前的可读输出。
/// 数据构造：1个运行轮次+1个排队请求+1段部分输出。
/// 执行过程：排队→开始→再排队→半段→服务失效。
/// 预期结果：没有永久转圈；错误可见；半段不会作为完成历史写入。
#[test]
fn service_retirement_stops_queued_and_active_feedback() {
    let mut state = TaskActivity::default();
    state.apply(0, TaskUpdate::Queued);
    state.apply(1, TaskUpdate::Started { message_count: 0 });
    state.apply(0, TaskUpdate::Queued);
    state.apply(
        1,
        TaskUpdate::Update(TraceItem::new(TraceKind::Reply, "", "部分")),
    );
    state.interrupt();
    assert!(!state.waiting(), "服务停止必须清掉所有等待");
    assert!(state.failed, "中断必须可见");
    assert_eq!(state.reply, "部分", "失败前输出保留给用户判断");
    assert_eq!(state.queued, 0, "不能残留排队计数");
}
