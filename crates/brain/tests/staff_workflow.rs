use brain::staff::TaskStore;
use norma::protocol::{StaffOp, Task, TaskDisplayStatus, TaskPhase, Worker};

/// 提议采用只授予一次派发；模型创建提议不能自行获得用户批准。
///
/// 数据构造：任务 a，一条 Simon 提议；proposal 编号从 1 开始，两个不同 dispatch d-1/d-2。
/// 执行过程：1. 模型提出提议。2. 未批准尝试绑定。3. 用户批准后绑定。4. 重复采用。
/// 预期结果：未批准无 Worker；批准后只有一个 Worker，重复不能派第二张卡；快照可重载。
#[test]
fn proposal_requires_user_adoption_and_dispatch_is_once() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: "Simon".into(),
                title: "Build".into(),
                prompt: "Build and test".into(),
                why: "Required".into(),
            },
        )
        .expect("propose");
    let pending = store.get("ws", "a").expect("pending");
    assert_eq!(
        pending.display_status(),
        TaskDisplayStatus::NeedsYou,
        "pending proposal must request a decision"
    );
    assert!(pending.workers.is_empty(), "proposing must not dispatch");
    assert!(
        store
            .bind_worker(
                "ws",
                "a",
                1,
                Worker::new("Simon", "Codex", "Build", 10, "d-1")
            )
            .is_err(),
        "model cannot adopt its own proposal"
    );
    store
        .approve("ws", "a", 1, None, None)
        .expect("user approval");
    store
        .bind_worker(
            "ws",
            "a",
            1,
            Worker::new("Simon", "Codex", "Build", 10, "d-1"),
        )
        .expect("bind first dispatch");
    assert!(
        store
            .bind_worker(
                "ws",
                "a",
                1,
                Worker::new("Simon", "Codex", "Build", 11, "d-2")
            )
            .is_err(),
        "replay cannot create another worker"
    );
    let restored = TaskStore::new(dir.path().to_owned())
        .get("ws", "a")
        .expect("restore");
    assert_eq!(
        restored.workers.len(),
        1,
        "one adopted proposal must bind exactly one Worker"
    );
    assert_eq!(
        restored.workers[0].card_id,
        Some(10),
        "first card must remain the owner"
    );
    assert_ne!(
        restored.workers[0].dispatch_id, "d-2",
        "second dispatch must not overwrite binding"
    );
    assert_eq!(
        restored.phase,
        TaskPhase::Active,
        "dispatch must keep task active"
    );
}

/// 用户采纳时改选的智能体和模型必须随批准落库，空白模型视为默认。
///
/// 数据构造：任务 a 上 Simon 的提议 1；用户改选 Kimi 与模型 " opus "。
/// 执行过程：先 propose，再 approve(Some("Kimi"), Some(" opus "))，重读任务。
/// 预期结果：proposal.agent == "Kimi"，model == Some("opus")，approved 为真。
#[test]
fn approval_persists_user_agent_and_model_choice() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: "Simon".into(),
                title: "Build".into(),
                prompt: "Build and test".into(),
                why: "Required".into(),
            },
        )
        .expect("propose");
    store
        .approve("ws", "a", 1, Some("Kimi".into()), Some(" opus ".into()))
        .expect("approve");
    let proposal = store.get("ws", "a").expect("task").proposals[0].clone();
    assert_eq!(proposal.agent, "Kimi");
    assert_eq!(proposal.model.as_deref(), Some("opus"));
    assert!(proposal.approved);
}
