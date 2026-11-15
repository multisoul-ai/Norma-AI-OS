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

fn propose(store: &TaskStore, agent: &str, prompt: &str) {
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: agent.into(),
                title: "T".into(),
                prompt: prompt.into(),
                why: "W".into(),
            },
        )
        .expect("propose");
}

/// 一轮：开始轮次、执行提议、轮末整批替换（与幕僚 lane 的调用顺序一致）。
fn round(store: &TaskStore, proposals: &[(&str, &str)]) -> Task {
    store.begin_round("ws", "a").expect("begin");
    for (agent, prompt) in proposals {
        propose(store, agent, prompt);
    }
    store
        .update("ws", "a", |task| {
            task.supersede_stale_proposals();
            Ok(())
        })
        .expect("end round")
}

fn proposal_messages(task: &Task) -> Vec<u64> {
    task.messages
        .iter()
        .filter_map(|m| match m {
            norma::protocol::TaskMessage::Staff(reply) => Some(reply),
            _ => None,
        })
        .flat_map(|reply| reply.parts.iter())
        .filter_map(|part| match part {
            norma::protocol::StaffPart::Proposal(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// 仍然有效（没有被新方案替换）的提议编号。
fn live_ids(task: &Task) -> Vec<u64> {
    task.proposals
        .iter()
        .filter(|p| !p.superseded)
        .map(|p| p.id)
        .collect()
}

fn fresh() -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    (dir, store)
}

/// 并行提议共存；下一轮只提新的 -> 旧的整批标记为已替换，卡片保留在原回合里，新 id 不与旧 id 重复。
#[test]
fn new_round_supersedes_pending_proposals_and_ids_stay_unique() {
    let (_dir, store) = fresh();
    let t = round(&store, &[("Simon", "p1"), ("Kimi", "p2")]);
    assert_eq!(live_ids(&t), [1, 2], "第一轮两个提议都有效");
    let t = round(&store, &[("Codex", "p3")]);
    assert_eq!(live_ids(&t), [3], "第二轮只剩新提议有效");
    assert_eq!(t.proposals.len(), 3, "旧提议不删除，只标记");
    assert_eq!(proposal_messages(&t), [1, 2, 3], "旧卡片引用保留在原回合");
    let t = round(&store, &[("Codex", "p4")]);
    assert_eq!(live_ids(&t), [4], "id 单调递增，不与旧 id 重复");
}

/// 原样重提保留原 id；本轮不提则旧提议不变；已 approved/派发的不被删。
#[test]
fn resubmitted_proposal_keeps_id_and_silent_round_changes_nothing() {
    let (_dir, store) = fresh();
    round(&store, &[("Simon", "p1"), ("Kimi", "p2")]);
    let t = round(&store, &[("Kimi", "p2"), ("Codex", "p3")]);
    assert_eq!(
        t.proposals
            .iter()
            .filter(|p| !p.superseded)
            .map(|p| (p.id, p.prompt.as_str()))
            .collect::<Vec<_>>(),
        [(2, "p2"), (3, "p3")],
        "原样重提的 p2 保留原 id 且仍有效，p1 被替换"
    );
    let silent = round(&store, &[]);
    assert_eq!(
        live_ids(&silent),
        [2, 3],
        "silent round keeps old proposals valid"
    );
}

#[test]
fn approved_and_dispatched_proposals_survive_replacement() {
    let (_dir, store) = fresh();
    round(&store, &[("Simon", "p1"), ("Kimi", "p2"), ("Codex", "p3")]);
    store.approve("ws", "a", 1, None, None).expect("approve");
    store.approve("ws", "a", 2, None, None).expect("approve");
    store
        .bind_worker("ws", "a", 2, Worker::new("Kimi", "Kimi", "r", 10, "d-1"))
        .expect("bind");
    let t = round(&store, &[("Codex", "p9")]);
    assert_eq!(
        live_ids(&t),
        [1, 2, 4],
        "approved(1) and dispatched(2) stay, pending(3) replaced"
    );
    assert!(
        store.approve("ws", "a", 3, None, None).is_err(),
        "superseded proposal cannot be adopted"
    );
}
