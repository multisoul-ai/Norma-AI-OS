use super::{sync_ideas, DispatchLookup, TaskLookup};
use std::collections::{HashMap, HashSet};
use storage::{DesktopWorkspaceStore, IdeaSource, IdeasStore, Reconcile};
use workspace::ideas::{IdeaState, TaskPhase};

/// 用内存表模拟“协调器 + 派发库”：phase 按 task_ref 查，sent 按正文查。
#[derive(Default)]
struct Fake {
    phases: HashMap<String, TaskPhase>,
    sent: HashMap<String, Vec<String>>,
    unreadable: bool,
}

impl TaskLookup for Fake {
    fn phase_of(&self, task_ref: &str) -> Option<TaskPhase> {
        self.phases.get(task_ref).copied()
    }
    fn find_sent(&self, body: &str, owned: &HashSet<String>) -> Reconcile {
        if self.unreadable {
            return Reconcile::Unknown;
        }
        match self
            .sent
            .get(body)
            .and_then(|refs| refs.iter().rev().find(|r| !owned.contains(*r)))
        {
            Some(task_ref) => Reconcile::Sent(task_ref.clone()),
            None => Reconcile::NotSent,
        }
    }
}

fn setup() -> (tempfile::TempDir, IdeasStore, String) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let ws = DesktopWorkspaceStore::new(db.clone())
        .create("空间")
        .expect("应创建")
        .id;
    (temp, IdeasStore::new(db), ws)
}

/// 场景：旧项目目录不可用时仍读取工作空间想法，不能误判未派发并回退。
/// 数据构造：一个派发中想法，关联一个已离线且尚无全局库的项目。
/// 执行过程：后台准备项目来源后，从真实 DispatchLookup 对账并读取清单。
/// 预期结果：清单仍包含原想法；无法查询的任务保持派发中，查找结果为 Unknown。
#[test]
fn unavailable_legacy_project_does_not_hide_ideas_or_reset_dispatch() {
    let (temp, store, ws) = setup();
    store
        .add(&ws, "pending", "处理离线项目", IdeaSource::Chat)
        .expect("应添加想法");
    store
        .begin_dispatch(&ws, "pending", "operation-a")
        .expect("应进入派发中")
        .expect("想法应存在");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let missing = temp.path().join("offline-project");
    let mut lookup = DispatchLookup {
        live: Vec::new(),
        project_dbs: vec![paths.project_db_path(&missing)],
        unavailable_projects: false,
    };
    lookup.prepare_projects(&paths, &[missing]);
    let ideas = sync_ideas(&store, &ws, &lookup, 1_000, 0).expect("离线项目不能阻断清单读取");

    assert_eq!(ideas.len(), 1, "工作空间想法不能因单个项目离线而消失");
    assert_eq!(
        ideas[0].state,
        IdeaState::Dispatching,
        "未知派发结果不能被回退到待办"
    );
    assert_eq!(
        lookup.find_sent("处理离线项目", &HashSet::new()),
        Reconcile::Unknown,
        "无法读取来源时只能返回未知"
    );
}

fn assign(store: &IdeasStore, ws: &str, id: &str, body: &str, task_ref: &str) {
    store.add(ws, id, body, IdeaSource::Chat).expect("应记下");
    store
        .begin_dispatch(ws, id, task_ref)
        .expect("不应出错")
        .expect("应开始派发");
    store
        .mark_assigned(ws, id, "area", 1, None)
        .expect("不应出错")
        .expect("应指派");
}

/// 重启后协调器是空的，完成判定要靠派发库：成功的任务让想法完成，且同步幂等。
///
/// 数据构造：两条已指派想法，引用 old-run:1（派发库里为成功）与 old-run:2（派发库里没有记录）。
/// 预期结果：
///   - 正断言：第一次同步后 a 为已完成；
///   - 负断言：查不到记录的 b 保持已指派（不被猜测完成）；第二次同步结果不变。
#[test]
fn sync_completes_from_dispatch_records_after_restart() {
    let (_temp, store, ws) = setup();
    assign(&store, &ws, "a", "甲", "old-run:1");
    assign(&store, &ws, "b", "乙", "old-run:2");
    let lookup = Fake {
        phases: HashMap::from([("old-run:1".to_owned(), TaskPhase::Succeeded)]),
        ..Fake::default()
    };
    let first = sync_ideas(&store, &ws, &lookup, 1_000, 0).expect("同步不应出错");
    let state = |list: &[storage::StoredIdea], id: &str| {
        list.iter().find(|i| i.id == id).expect("应存在").state
    };
    assert_eq!(
        state(&first, "a"),
        IdeaState::Done,
        "成功的任务应让想法完成"
    );
    assert_eq!(
        state(&first, "b"),
        IdeaState::Assigned,
        "查不到记录的想法不应被猜测完成"
    );
    let second = sync_ideas(&store, &ws, &lookup, 1_000, 0).expect("同步不应出错");
    assert_eq!(
        (state(&second, "a"), state(&second, "b")),
        (IdeaState::Done, IdeaState::Assigned),
        "重复同步应幂等"
    );
}

/// 对账：按正文找已发出的任务，但已被别的想法占用的引用不能再认领；读取失败时保持不动。
///
/// 数据构造：想法 a 已完成并持有 run:1（正文“修复登录”）；想法 b 同文滞留派发中。
/// 执行过程：派发库里同文任务只有 run:1 → 同步；再加入 run:2 → 同步；最后让读取失败。
/// 预期结果：
///   - 负断言：只有 run:1 时 b 不能认领它（已被 a 占用）→ 视为没发出回待办；
///   - 正断言：（重新派发回派发中后）run:2 出现时 b 认领 run:2 变为已指派；
///   - 负断言：读取失败（Unknown）时 b 保持派发中，不回退。
#[test]
fn sync_reconcile_never_claims_owned_refs_and_keeps_unknown() {
    let (_temp, store, ws) = setup();
    assign(&store, &ws, "a", "修复登录", "run:1");
    assert!(
        store.complete_by_task_ref("run:1").expect("不应出错"),
        "a 应完成"
    );
    store
        .add(&ws, "b", "修复登录", IdeaSource::Chat)
        .expect("应记下");
    store
        .begin_dispatch(&ws, "b", "idea:b")
        .expect("不应出错")
        .expect("应开始派发");
    let only_old = Fake {
        sent: HashMap::from([("修复登录".to_owned(), vec!["run:1".to_owned()])]),
        ..Fake::default()
    };
    let unreadable = Fake {
        unreadable: true,
        ..Fake::default()
    };
    let state_b =
        |list: Vec<storage::StoredIdea>| list.into_iter().find(|i| i.id == "b").expect("应存在");
    assert_eq!(
        state_b(sync_ideas(&store, &ws, &unreadable, 1_000, 0).expect("不应出错")).state,
        IdeaState::Dispatching,
        "读取失败时应保持派发中"
    );
    let after_old = state_b(sync_ideas(&store, &ws, &only_old, 1_000, 0).expect("不应出错"));
    assert_eq!(
        (after_old.state, after_old.task_ref),
        (IdeaState::Todo, None),
        "run:1 已被 a 占用，b 不能认领，应视为没发出"
    );
    store
        .begin_dispatch(&ws, "b", "idea:b")
        .expect("不应出错")
        .expect("应再次派发");
    let with_new = Fake {
        sent: HashMap::from([(
            "修复登录".to_owned(),
            vec!["run:1".to_owned(), "run:2".to_owned()],
        )]),
        ..Fake::default()
    };
    let claimed = state_b(sync_ideas(&store, &ws, &with_new, 1_000, 0).expect("不应出错"));
    assert_eq!(
        (claimed.state, claimed.task_ref.as_deref()),
        (IdeaState::Assigned, Some("run:2")),
        "应认领未被占用的 run:2"
    );
}

/// 占位引用被替换：派发成功但当时没拿到真实引用的想法，同步时按正文找回。
///
/// 数据构造：已指派想法持有占位 idea:a；派发库里同文任务 run:9。
/// 预期结果：
///   - 正断言：同步后 task_ref 为 run:9，之后可按其阶段完成。
///   - 负断言：占位引用不再残留。
#[test]
fn sync_replaces_placeholder_refs_so_assigned_ideas_can_finish() {
    let (_temp, store, ws) = setup();
    assign(&store, &ws, "a", "写文档", "idea:a");
    let lookup = Fake {
        sent: HashMap::from([("写文档".to_owned(), vec!["run:9".to_owned()])]),
        phases: HashMap::from([("run:9".to_owned(), TaskPhase::Succeeded)]),
        ..Fake::default()
    };
    let list = sync_ideas(&store, &ws, &lookup, 1_000, 0).expect("不应出错");
    assert_eq!(
        list[0].state,
        IdeaState::Done,
        "替换为真实引用后应能按阶段完成"
    );
    assert_eq!(
        list[0].task_ref.as_deref(),
        Some("run:9"),
        "占位引用应被真实引用替换"
    );
}
