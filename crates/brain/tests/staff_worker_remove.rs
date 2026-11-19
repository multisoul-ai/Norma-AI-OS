use brain::staff::policy::{authorize_staff, Verdict};
use brain::staff::TaskStore;
use norma::protocol::{BrainOp, StaffOp, Task, Worker};

fn store_with_workers() -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Snake"))
        .expect("create");
    store
        .update("ws", "a", |task| {
            task.workers
                .push(Worker::new("AgentC", "OpenCode", "Build", 10, "d-1"));
            task.workers
                .push(Worker::new("AgentA", "Codex", "Test", 11, "d-2"));
            Ok(())
        })
        .expect("workers");
    (dir, store)
}

fn op(kind: &str, worker: &str) -> StaffOp {
    let (task_id, worker) = ("a".to_owned(), worker.to_owned());
    match kind {
        "remove" => StaffOp::RemoveWorker { task_id, worker },
        _ => StaffOp::RestoreWorker { task_id, worker },
    }
}

/// 场景：用户移除再撤销一个 Worker，改动要落盘。
/// 数据构造：任务里有 d-1、d-2 两个 Worker；移除 d-1 后重新读盘，再恢复。
/// 预期结果：重新读盘只剩 d-2；恢复后 d-1 回到第一位；对不存在的 Worker 报 staff.worker_unknown。
#[test]
fn remove_and_restore_persist() {
    let (_dir, store) = store_with_workers();
    store.execute("ws", op("remove", "d-1")).expect("remove");
    let ids = |store: &TaskStore| -> Vec<String> {
        store
            .get("ws", "a")
            .expect("task")
            .workers
            .into_iter()
            .map(|w| w.dispatch_id)
            .collect()
    };
    assert_eq!(ids(&store), ["d-2"], "check: after remove");
    store.execute("ws", op("restore", "d-1")).expect("restore");
    assert_eq!(ids(&store), ["d-1", "d-2"], "check: after restore");
    let error = store
        .execute("ws", op("remove", "nope"))
        .expect_err("unknown");
    assert_eq!(error.to_string(), "staff.worker_unknown", "check: error");
}

/// 场景：幕僚（模型）尝试移除或恢复 Worker。
/// 数据构造：两条命令分别经幕僚授权。
/// 预期结果：都被拒绝，拒绝码是 staff.denied.remove_worker。
#[test]
fn staff_cannot_remove_workers_yet() {
    for kind in ["remove", "restore"] {
        let verdict = authorize_staff(
            &BrainOp::Task {
                op: op(kind, "d-1"),
            },
            false,
        );
        assert_eq!(
            verdict,
            Verdict::Denied {
                code: "staff.denied.remove_worker"
            },
            "{kind} must be denied for staff"
        );
    }
}
