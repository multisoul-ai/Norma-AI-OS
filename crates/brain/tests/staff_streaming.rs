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

/// 场景：用户停止当前幕僚回复后，同一个任务仍能继续对话。
/// 数据构造：假 ACP 发出第一段后等文件放行；通过真实服务重新发送第二轮。
/// 执行过程：等第一段可见 → 停止当前轮次 → 读回落盘结果 → 放行适配器 → 发送下一轮。
/// 预期结果：第一轮保留可见文字、状态为 Stopped，迟到的第二段不会进入旧回复；下一轮正常完成。
#[test]
fn stopping_a_staff_turn_keeps_partial_text_and_allows_another_turn() {
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
    config.env = vec![
        ("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string()),
        ("FAKE_RELEASE".into(), release.display().to_string()),
    ];
    let (service, statuses) = BrainService::start(config);
    let store = service.tasks();
    let scope = BrainScope {
        workspace_id: "ws".into(),
        route_id: "route".into(),
    };
    let mut task = Task::new("task-a", "ws", "project", "Interrupt");
    task.user_message("FAKE_STREAM_HOLD");
    store.create(task).expect("task persisted");
    let request = || {
        service.push(
            BrainEventKind::TaskRequest {
                task_id: "task-a".into(),
            }
            .in_scope(scope.clone()),
        );
    };
    let wait_for = |matches: &dyn Fn(&BrainStatusKind) -> bool| {
        // 5s 包含本地握手和线程调度。第一轮在放行前只能通过取消收尾。
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Ok(status) =
            statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            if matches(&status.kind) {
                return true;
            }
        }
        false
    };
    request();
    let visible = wait_for(
        &|kind| matches!(kind, BrainStatusKind::TaskReplyChanged { reply, .. } if reply.text() == "第一段"),
    );
    let round = store.get("ws", "task-a").expect("running task").round;
    let wrong_route = BrainScope {
        route_id: "other".into(),
        ..scope.clone()
    };
    let wrong_route_stopped = service.stop_staff_turn(&wrong_route, "task-a", round);
    let stale_round_stopped = service.stop_staff_turn(&scope, "task-a", round.saturating_sub(1));
    let accepted = service.stop_staff_turn(&scope, "task-a", round);
    let stopped = wait_for(&|kind| {
        matches!(kind, BrainStatusKind::TaskChanged(task)
            if matches!(task.messages.last(), Some(TaskMessage::Staff(reply)) if reply.status == ReplyStatus::Stopped))
    });
    let durable = std::fs::read_to_string(store.path("ws")).expect("stopped history");
    std::fs::write(&release, "go").expect("release any stale ACP response");
    store
        .update("ws", "task-a", |task| {
            task.user_message("下一轮");
            Ok(())
        })
        .expect("follow-up");
    request();
    let completed = wait_for(&|kind| {
        matches!(kind, BrainStatusKind::TaskChanged(task)
            if task.round == round + 1
              && matches!(task.messages.last(), Some(TaskMessage::Staff(reply)) if reply.status == ReplyStatus::Done))
    });
    let final_task = store.get("ws", "task-a").expect("task after follow-up");
    service.shutdown();
    assert!(
        visible && accepted && stopped,
        "可见的第一段必须在放行前被用户停止"
    );
    assert!(
        !wrong_route_stopped && !stale_round_stopped,
        "取消只能针对相同路由及轮次"
    );
    assert!(durable.contains("第一段"), "停止时已显示的内容必须持久化");
    assert!(
        !durable.contains("第二段"),
        "停止后不得把未显示的内容写入旧回复"
    );
    assert!(completed, "下一轮必须正常完成");
    let replies: Vec<_> = final_task
        .messages
        .iter()
        .filter_map(|message| match message {
            TaskMessage::Staff(reply) => Some((reply.status, reply.text())),
            _ => None,
        })
        .collect();
    assert_eq!(
        replies,
        vec![
            (ReplyStatus::Stopped, "第一段".to_owned()),
            (ReplyStatus::Done, "第一段第二段".to_owned())
        ],
        "迟到的通知不得串到旧回复或下一轮"
    );
}
