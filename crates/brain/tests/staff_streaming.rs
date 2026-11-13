use brain::acp::trace::TraceKind;
use brain::event::TaskUpdate;
use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService, BrainStatusKind};
use norma::protocol::{BrainScope, Task, TaskMessage};
use std::time::{Duration, Instant};

/// 场景：任务在模型结束之前展示部分回复和计划，最终仅保存一条完整回复。
///
/// 数据构造：task-a 一条用户消息；假 ACP 发“第一段”和两项计划，等待文件放行再发“第二段”。
/// 执行过程：发送 TaskRequest → 接收活动 → 在收到第一段后放行 → 等最终 TaskChanged。
/// 预期结果：放行前有 queued/started/plan/chunk，历史没有半条回复；放行后完整回复仅一条。
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
    let (mut queued, mut started, mut plan, mut partial) = (false, false, false, false);
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if let BrainStatusKind::TaskActivity {
            task_id, update, ..
        } = status.kind
        {
            assert_eq!(task_id, "task-a", "活动必须关联原任务");
            match update {
                TaskUpdate::Queued => queued = true,
                TaskUpdate::Started { message_count } => {
                    assert_eq!(message_count, 1, "开始事件基线来自真实任务的一条用户消息");
                    started = true;
                }
                TaskUpdate::Update(item) if item.kind == TraceKind::Plan => {
                    plan = item.text.contains("[in_progress] 整理建议")
                }
                TaskUpdate::Update(item) if item.kind == TraceKind::Reply => {
                    partial = item.text == "第一段";
                    break;
                }
                _ => {}
            }
        }
    }
    let before = store.get("ws", "task-a").expect("task before finish");
    std::fs::write(&release, "go").expect("release fixture even on failed assertion");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished = None;
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            finished = Some(task);
            break;
        }
    }
    service.shutdown();
    assert!(queued, "入队必须立即产生等待反馈");
    assert!(started, "开始握手/请求必须产生运行反馈");
    assert!(plan, "真实 ACP 计划必须在完成前可见");
    assert!(partial, "第一段必须在模型结束之前可见");
    assert!(
        !before
            .messages
            .iter()
            .any(|m| matches!(m, TaskMessage::Norma { .. })),
        "中间输出不得写成历史回复"
    );
    let task = finished.expect("completed task snapshot");
    assert_eq!(
        task.messages.last(),
        Some(&TaskMessage::Norma {
            text: "第一段第二段".into()
        }),
        "最终必须保存完整回复"
    );
    assert_eq!(
        task.messages
            .iter()
            .filter(|m| matches!(m, TaskMessage::Norma { .. }))
            .count(),
        1,
        "最终回复不可重复"
    );
}
