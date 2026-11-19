use brain::event::TaskUpdate;
use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService, BrainStatusKind};
use norma::protocol::{BrainScope, ReplyStatus, ReplyTrigger, StaffPart, Task, TaskMessage};
use std::time::{Duration, Instant};

/// 场景：任务在模型结束之前展示部分回复和计划，最终仍是同一条完整回复。
///
/// 数据构造：task-a 一条用户消息；假 ACP 发“第一段”和两项计划，等待文件放行再发“第二段”。
/// 执行过程：发送 TaskRequest → 接收活动 → 在收到第一段后放行 → 等最终 TaskChanged。
/// 预期结果：放行前有 queued/started 与带计划的增量，内存有半条回复但磁盘没有；放行后完整回复仅一条且 Done。
#[test]
fn task_stream_is_visible_before_completion_without_partial_history() {
    let dir = tempfile::tempdir().expect("isolated state");
    let release = dir.path().join("release");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![script.display().to_string()],
        },
        "/opt/norma".into(),
        dir.path().into(),
        dir.path().join("state"),
    );
    config.router.summary_every = None;
    config.env = vec![
        ("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string()),
        ("FAKE_RELEASE".into(), release.display().to_string()),
    ];
    let (service, statuses) = BrainService::start(config);
    let store = service.tasks();
    let mut task = Task::new("task-a", "ws", "project", "Streaming");
    task.user_message("FAKE_STREAM_HOLD");
    store.create(task).expect("task persisted");
    service.push(
        BrainEventKind::TaskRequest {
            task_id: "task-a".into(),
        }
        .in_scope(BrainScope {
            workspace_id: "ws".into(),
            route_id: "route".into(),
        }),
    );
    // 5s = 本地 Python 握手及线程调度预算；完成依赖 release，不能靠睡眠让假阳性通过。
    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut queued, mut started, mut partial) = (false, false, None);
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        match status.kind {
            BrainStatusKind::TaskActivity {
                task_id, update, ..
            } => {
                assert_eq!(task_id, "task-a", "活动必须关联原任务");
                match update {
                    TaskUpdate::Queued => queued = true,
                    TaskUpdate::Started => started = true,
                    _ => {}
                }
            }
            BrainStatusKind::TaskReplyChanged { index, reply, .. }
                if reply.text().contains("第一段") =>
            {
                partial = Some((index, reply));
                break;
            }
            _ => {}
        }
    }
    let before = store.get("ws", "task-a").expect("task before finish");
    let on_disk = std::fs::read_to_string(store.path("ws")).expect("snapshot on disk");
    std::fs::write(&release, "go").expect("release fixture even on failed assertion");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished = None;
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            if matches!(task.messages.last(), Some(TaskMessage::Staff(r)) if !r.is_running()) {
                finished = Some(task);
                break;
            }
        }
    }
    service.shutdown();
    assert!(queued, "入队必须立即产生等待反馈");
    assert!(started, "开始握手/请求必须产生运行反馈");
    let (index, reply) = partial.expect("第一段必须在模型结束之前通过增量事件可见");
    assert_eq!(index, 1, "增量指向用户消息之后的那条回复");
    assert!(reply.is_running(), "流式中的回复处于 Running");
    assert!(
        reply
            .parts
            .iter()
            .any(|p| matches!(p, StaffPart::Plan(items) if items.iter().any(|i| i.text == "整理建议" && i.status == "in_progress"))),
        "真实 ACP 计划必须在完成前进入回复"
    );
    assert!(
        matches!(before.messages.last(), Some(TaskMessage::Staff(r)) if r.text().contains("第一段")),
        "内存里能立刻读到半条回复"
    );
    assert!(!on_disk.contains("第一段"), "流式文字不得提前写盘");
    let task = finished.expect("completed task snapshot");
    let replies: Vec<_> = task
        .messages
        .iter()
        .filter_map(|m| match m {
            TaskMessage::Staff(reply) => Some(reply),
            _ => None,
        })
        .collect();
    assert_eq!(replies.len(), 1, "一轮只有一条幕僚回复");
    assert_eq!(replies[0].text(), "第一段第二段", "最终保存完整回复");
    assert_eq!(replies[0].status, ReplyStatus::Done, "回复以 Done 结束");
    assert_eq!(
        replies[0].trigger,
        ReplyTrigger::User,
        "用户请求触发的回复可重新生成"
    );
}
