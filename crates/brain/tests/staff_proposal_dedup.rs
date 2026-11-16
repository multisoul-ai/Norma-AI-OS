use brain::staff::TaskStore;
use norma::protocol::{StaffOp, Task};

fn propose(agent: &str, title: &str, prompt: &str) -> StaffOp {
    StaffOp::Propose {
        task_id: "a".into(),
        agent: agent.into(),
        title: title.into(),
        prompt: prompt.into(),
        why: "Required".into(),
        model: None,
    }
}

fn new_store() -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    (dir, store)
}

/// [场景描述]：同一回合内，同 agent、标题只差大小写/空白/全角的三条提议，只保留最新一条待采用。
///
/// 数据构造：
///   提议 1：agent=Bella，title="Fix Staff Layout"，prompt="A"
///   提议 2：agent=Bella，title="fix  staff layout"（小写 + 双空格），prompt="B"
///   提议 3：agent=Bella，title="ＦＩＸ staff layout"（全角大写），prompt="C"
///   三条 prompt 互不相同，所以现有「agent+prompt 全文相同」的去重不会命中。
///   三条都在同一回合（round=0），所以现有 `supersede_stale_proposals`（只替换更早轮次）不会命中。
///
/// 执行过程：
///   1. 依次 execute 三条 Propose → 各自入库，id 分别为 1、2、3
///   2. 第 2 条入库时，id=1 同 key 且未采用 → 标记 superseded
///   3. 第 3 条入库时，id=2 同 key 且未采用 → 标记 superseded
///
/// 预期结果：
///   - id=1、id=2 为 superseded，否则用户会看到三张待采用卡
///   - id=3 不是 superseded，是唯一待采用的提议
#[test]
fn same_round_proposals_with_the_same_normalized_title_keep_only_the_latest() {
    let (_dir, store) = new_store();
    for (title, prompt) in [
        ("Fix Staff Layout", "A"),
        ("fix  staff layout", "B"),
        ("ＦＩＸ staff layout", "C"),
    ] {
        store
            .execute("ws", propose("Bella", title, prompt))
            .expect("propose");
    }
    let task = store.get("ws", "a").expect("get");
    let flag = |id: u64| {
        task.proposals
            .iter()
            .find(|p| p.id == id)
            .expect("proposal exists")
            .superseded
    };
    assert!(
        flag(1),
        "proposal 1 must be superseded by the later same-title proposal"
    );
    assert!(
        flag(2),
        "proposal 2 must be superseded by the later same-title proposal"
    );
    assert!(!flag(3), "proposal 3 is the latest and must stay pending");
}

/// [场景描述]：标题不同，或 agent 不同，互不覆盖；已派发的旧提议也不被标记。
///
/// 数据构造：
///   提议 1：Bella / "Build"，已采用并绑定 Worker（dispatch_id 存在）
///   提议 2：Bella / "Test"（标题不同）
///   提议 3：Simon / "Test"（agent 不同，标题与 2 相同）
///   提议 4：Bella / "Build"（与已派发的 1 同 key）
///
/// 预期结果：
///   - id=1 已派发，不得被标记 superseded
///   - id=2 不被 3 覆盖（agent 不同），保持待采用
///   - id=3、id=4 都是各自 key 下最新的，保持待采用
#[test]
fn different_titles_or_agents_do_not_supersede_and_dispatched_are_untouched() {
    let (_dir, store) = new_store();
    store
        .execute("ws", propose("Bella", "Build", "p1"))
        .expect("p1");
    store.approve("ws", "a", 1, None, None).expect("approve");
    store
        .bind_worker(
            "ws",
            "a",
            1,
            norma::protocol::Worker::new("Bella", "Claude", "Build", 10, "d-1"),
        )
        .expect("bind");
    store
        .execute("ws", propose("Bella", "Test", "p2"))
        .expect("p2");
    store
        .execute("ws", propose("Simon", "Test", "p3"))
        .expect("p3");
    store
        .execute("ws", propose("Bella", "Build", "p4"))
        .expect("p4");
    let task = store.get("ws", "a").expect("get");
    let flag = |id: u64| {
        task.proposals
            .iter()
            .find(|p| p.id == id)
            .expect("proposal exists")
            .superseded
    };
    assert!(!flag(1), "a dispatched proposal must never be superseded");
    assert!(
        !flag(2),
        "same title but different agent must not supersede"
    );
    assert!(!flag(3), "latest of its own key stays pending");
    assert!(!flag(4), "latest of its own key stays pending");
}
