use norma::protocol::{Task, TaskDisplayStatus, Worker, WorkerOutcome};

fn task_with(workers: &[(&str, &str)]) -> Task {
    let mut task = Task::new("a", "ws", "project", "Snake");
    for (index, (agent, dispatch)) in workers.iter().enumerate() {
        task.workers.push(Worker::new(
            agent,
            "OpenCode",
            "Build",
            10 + index as u64,
            dispatch,
        ));
    }
    task
}

/// 场景：用户移除其中一个 Worker，状态判断不再计入它；撤销后原位恢复。
/// 数据构造：三个 Worker（AgentC 未取得结果、AgentA 完成、AgentC 执行中）；移除第一个再恢复。
/// 预期结果：移除后只剩两个且顺序不变，被移除的进 removed_workers；恢复后回到原来的下标。
#[test]
fn removing_and_restoring_a_worker_keeps_its_place() {
    let mut task = task_with(&[("AgentC", "d-1"), ("AgentA", "d-2"), ("AgentC", "d-3")]);
    assert!(task.remove_worker("d-1"), "移除存在的 Worker 返回 true");
    let left: Vec<_> = task
        .workers
        .iter()
        .map(|w| w.dispatch_id.as_str())
        .collect();
    assert_eq!(left, ["d-2", "d-3"], "check: remaining workers");
    assert_eq!(task.removed_workers.len(), 1, "被移除的记录要留着，供撤销");
    assert!(task.restore_worker("d-1"), "恢复返回 true");
    let back: Vec<_> = task
        .workers
        .iter()
        .map(|w| w.dispatch_id.as_str())
        .collect();
    assert_eq!(back, ["d-1", "d-2", "d-3"], "check: restored order");
    assert!(task.removed_workers.is_empty(), "恢复后不再留在回收记录里");
}

/// 场景：移除或恢复一个不存在的 Worker。
/// 数据构造：一个 Worker d-1；移除 nope，恢复 nope。
/// 预期结果：都返回 false，任务不变。
#[test]
fn unknown_worker_is_a_no_op() {
    let mut task = task_with(&[("AgentC", "d-1")]);
    assert!(!task.remove_worker("nope"), "check: remove unknown");
    assert!(!task.restore_worker("nope"), "check: restore unknown");
    assert_eq!(task.workers.len(), 1, "check: workers");
}

/// 场景：只有一个出了问题的 Worker，移除后任务不再要求用户处理。
/// 数据构造：一个结果为 Unavailable 的 Worker，任务状态为「需要你」。
/// 预期结果：移除后状态变为不再需要用户处理（Waiting）。
#[test]
fn removed_worker_no_longer_needs_the_user() {
    let mut task = task_with(&[("AgentC", "d-1")]);
    task.finish_worker(10, "d-1", WorkerOutcome::Unavailable, "nothing");
    assert_eq!(
        task.display_status(),
        TaskDisplayStatus::NeedsYou,
        "check: before"
    );
    task.remove_worker("d-1");
    assert_eq!(
        task.display_status(),
        TaskDisplayStatus::Waiting,
        "check: after"
    );
}

/// 场景：旧快照没有 removed_workers 字段。
/// 数据构造：把新任务序列化后去掉该字段再读回。
/// 预期结果：读取成功，回收记录为空。
#[test]
fn old_snapshots_load_without_removed_workers() {
    let mut value = serde_json::to_value(Task::new("a", "ws", "p", "t")).expect("json");
    value
        .as_object_mut()
        .expect("object")
        .remove("removed_workers");
    let task: Task = serde_json::from_value(value).expect("load");
    assert!(task.removed_workers.is_empty(), "check: removed_workers");
}
