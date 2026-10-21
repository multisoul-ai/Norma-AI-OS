use std::sync::{Arc, Barrier};
use storage::{DesktopWorkspaceStore, IdeaSource, IdeasStore};
use workspace::ideas::IdeaState;

fn setup() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("空间")
        .expect("应创建工作空间");
    (temp, db, workspace.id)
}

/// 想法按工作空间隔离、保持记录顺序、重建 Store 后仍在，删除的想法不出现在清单。
///
/// 数据构造：工作空间 A 依次记 甲/乙/丙，工作空间 B 记 丁；删除乙。
/// 执行过程：重建 Store 后分别列出 A 与 B。
/// 预期结果：
///   - 正断言：A 为 [甲, 丙]（顺序保持）；B 只有丁。
///   - 负断言：A 不含丁；乙不在清单；空白正文被拒绝且不落库。
#[test]
fn ideas_are_scoped_ordered_durable_and_hide_dismissed() {
    let (_temp, db, a) = setup();
    let b = DesktopWorkspaceStore::new(db.clone())
        .create("另一个")
        .expect("应创建 B")
        .id;
    let store = IdeasStore::new(db.clone());
    for (id, body) in [("i1", "甲"), ("i2", "乙"), ("i3", "丙")] {
        store
            .add(&a, id, body, IdeaSource::Chat)
            .expect("应记下想法");
    }
    store
        .add(&b, "i4", "丁", IdeaSource::Voice)
        .expect("B 应记下想法");
    assert!(
        store.add(&a, "i5", "  ", IdeaSource::Chat).is_err(),
        "空白正文应被拒绝"
    );
    assert!(
        store.dismiss(&a, "i2").expect("删除不应出错").is_some(),
        "待办应可删除"
    );

    let reopened = IdeasStore::new(db);
    let bodies = |id: &str| -> Vec<String> {
        reopened
            .list(id)
            .expect("应列出")
            .into_iter()
            .map(|idea| idea.body)
            .collect()
    };
    assert_eq!(bodies(&a), ["甲", "丙"], "A 应保持记录顺序并隐藏已删除的乙");
    assert_eq!(bodies(&b), ["丁"], "B 只应看到自己的想法");
    assert!(
        reopened
            .list(&a)
            .expect("应列出")
            .iter()
            .all(|idea| idea.state == IdeaState::Todo),
        "新想法都应是待办"
    );
}

/// 删除后撤销应回到原位置；已进入派发的想法不能被删除。
///
/// 数据构造：甲乙丙三条；删除乙再恢复；对丙开始派发后尝试删除。
/// 预期结果：
///   - 正断言：恢复后顺序仍是 [甲, 乙, 丙]。
///   - 负断言：派发中的丙 dismiss 返回 None 且仍在清单。
#[test]
fn restore_returns_to_original_position_and_dispatching_cannot_be_dismissed() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    for (id, body) in [("i1", "甲"), ("i2", "乙"), ("i3", "丙")] {
        store
            .add(&ws, id, body, IdeaSource::Manual)
            .expect("应记下想法");
    }
    store
        .dismiss(&ws, "i2")
        .expect("删除不应出错")
        .expect("待办应可删除");
    store
        .restore(&ws, "i2")
        .expect("恢复不应出错")
        .expect("已删除应可恢复");
    let order = |s: &IdeasStore| -> Vec<String> {
        s.list(&ws)
            .expect("应列出")
            .into_iter()
            .map(|i| i.body)
            .collect()
    };
    assert_eq!(order(&store), ["甲", "乙", "丙"], "恢复后应回到原位置");
    store
        .begin_dispatch(&ws, "i3", "task-3")
        .expect("开始派发不应出错")
        .expect("待办应可开始派发");
    assert!(
        store.dismiss(&ws, "i3").expect("删除不应出错").is_none(),
        "派发中的想法不能被删除"
    );
    assert_eq!(order(&store).len(), 3, "派发中的想法仍应在清单里");
}

/// 派发的比较并交换：并发放手只会成功一次，task_ref 在派发前就已落库。
///
/// 数据构造：一条待办；8 个线程同时对它 begin_dispatch，各用不同 task_ref。
/// 预期结果：
///   - 正断言：恰好 1 个线程拿到 Some；库里 task_ref 等于胜者的值且状态为派发中。
///   - 负断言：其余 7 个线程都拿到 None，不覆盖 task_ref。
#[test]
fn begin_dispatch_succeeds_exactly_once_under_contention() {
    let (_temp, db, ws) = setup();
    IdeasStore::new(db.clone())
        .add(&ws, "i1", "甲", IdeaSource::Chat)
        .expect("应记下想法");
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let (db, ws, barrier) = (db.clone(), ws.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                IdeasStore::new(db)
                    .begin_dispatch(&ws, "i1", &format!("task-{n}"))
                    .expect("并发开始派发不应出错")
                    .map(|_| format!("task-{n}"))
            })
        })
        .collect();
    let winners: Vec<String> = handles
        .into_iter()
        .filter_map(|h| h.join().expect("线程不应崩溃"))
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "8 个并发请求应恰好 1 个成功，实际 {winners:?}"
    );
    let idea = IdeasStore::new(db).list(&ws).expect("应列出").remove(0);
    assert_eq!(idea.state, IdeaState::Dispatching, "胜者之后应处于派发中");
    assert_eq!(
        idea.task_ref.as_deref(),
        Some(winners[0].as_str()),
        "task_ref 应等于胜者写入的值"
    );
}

/// 派发成功、失败回退、完成与撤回的存储语义。
///
/// 数据构造：两条想法；第一条派发成功并记录落点，随后按 task_ref 完成；第二条派发失败。
/// 预期结果：
///   - 正断言：成功后为已指派并带 area/node；complete_by_task_ref 置已完成；失败后回待办且清除 task_ref。
///   - 负断言：对未知 task_ref 完成返回 false；已完成想法不能再撤回。
#[test]
fn assignment_completion_failure_and_retract_follow_state_machine() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    store
        .add(&ws, "i1", "甲", IdeaSource::Chat)
        .expect("应记下想法");
    store
        .add(&ws, "i2", "乙", IdeaSource::Chat)
        .expect("应记下想法");
    store
        .begin_dispatch(&ws, "i1", "idea:i1")
        .expect("不应出错")
        .expect("应开始派发（占位 task_ref）");
    let assigned = store
        .mark_assigned(&ws, "i1", "area-1", 7, Some("run-9:41"))
        .expect("不应出错")
        .expect("派发中应可标记成功");
    assert_eq!(assigned.state, IdeaState::Assigned, "派发成功后应为已指派");
    assert_eq!(
        (
            assigned.assignee_area_id.as_deref(),
            assigned.assignee_node_id
        ),
        (Some("area-1"), Some(7)),
        "应记录落点智能体"
    );
    assert_eq!(
        assigned.task_ref.as_deref(),
        Some("run-9:41"),
        "派发回执的真实 task_ref（含冒号）应替换占位值"
    );
    assert!(
        !store
            .complete_by_task_ref("task-unknown")
            .expect("不应出错"),
        "未知 task_ref 不应完成任何想法"
    );
    assert!(
        !store.complete_by_task_ref("idea:i1").expect("不应出错"),
        "被替换掉的占位 task_ref 不应再匹配"
    );
    assert!(
        store.complete_by_task_ref("run-9:41").expect("不应出错"),
        "已知 task_ref 应完成对应想法"
    );
    assert!(
        store.retract(&ws, "i1").expect("不应出错").is_none(),
        "已完成的想法不能撤回"
    );

    store
        .begin_dispatch(&ws, "i2", "task-2")
        .expect("不应出错")
        .expect("应开始派发");
    let failed = store
        .fail_dispatch(&ws, "i2")
        .expect("不应出错")
        .expect("派发中应可回退");
    assert_eq!(
        (failed.state, failed.task_ref),
        (IdeaState::Todo, None),
        "失败后应回待办并清除 task_ref"
    );
}

/// 重启后的对账：遗留的派发中想法，按 task_ref 是否真的发出来决定去向，不盲目回退。
///
/// 数据构造：甲已发出（task-1 在已知集合里）、乙未发出（task-2 不在）；两者都停在派发中。
/// 预期结果：
///   - 正断言：甲变为已指派且保留 task_ref；乙回待办并清除 task_ref。
///   - 负断言：对账不触碰待办与其他工作空间。
#[test]
fn reconcile_resolves_dispatching_by_task_ref_and_leaves_others_alone() {
    let (_temp, db, ws) = setup();
    let other = DesktopWorkspaceStore::new(db.clone())
        .create("其他")
        .expect("应创建")
        .id;
    let store = IdeasStore::new(db);
    for (id, body) in [("i1", "甲"), ("i2", "乙"), ("i3", "丙")] {
        store
            .add(&ws, id, body, IdeaSource::Chat)
            .expect("应记下想法");
    }
    store
        .add(&other, "i9", "他", IdeaSource::Chat)
        .expect("应记下想法");
    store
        .begin_dispatch(&ws, "i1", "task-1")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .begin_dispatch(&ws, "i2", "task-2")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .begin_dispatch(&other, "i9", "task-9")
        .expect("不应出错")
        .expect("应开始派发");

    let changed = store
        .reconcile_dispatching(&ws, |task_ref| task_ref == "task-1")
        .expect("对账不应出错");
    assert_eq!(changed, 2, "只有本工作空间的两条派发中想法被处理");
    let by_id = |id: &str| {
        store
            .list(&ws)
            .expect("应列出")
            .into_iter()
            .find(|i| i.id == id)
            .expect("想法应存在")
    };
    assert_eq!(
        by_id("i1").state,
        IdeaState::Assigned,
        "已发出的应补成已指派"
    );
    assert_eq!(
        by_id("i1").task_ref.as_deref(),
        Some("task-1"),
        "已指派应保留 task_ref"
    );
    assert_eq!(
        (by_id("i2").state, by_id("i2").task_ref),
        (IdeaState::Todo, None),
        "未发出的应回待办并清除 task_ref"
    );
    assert_eq!(by_id("i3").state, IdeaState::Todo, "待办不应被对账改动");
    assert_eq!(
        store.list(&other).expect("应列出")[0].state,
        IdeaState::Dispatching,
        "其他工作空间的派发中想法不应被对账"
    );
}

/// 删除工作空间时想法级联删除。
///
/// 预期结果：
///   - 正断言：删除前能列出；删除后清单为空。
///   - 负断言：删除后不能再向已不存在的工作空间记想法。
#[test]
fn deleting_workspace_removes_its_ideas() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db.clone());
    store
        .add(&ws, "i1", "甲", IdeaSource::Chat)
        .expect("应记下想法");
    assert_eq!(
        store.list(&ws).expect("应列出").len(),
        1,
        "删除前应有一条想法"
    );
    assert!(
        DesktopWorkspaceStore::new(db)
            .delete(&ws)
            .expect("应删除工作空间"),
        "工作空间应被删除"
    );
    assert!(
        store.list(&ws).expect("应列出").is_empty(),
        "工作空间删除后想法应级联删除"
    );
    assert!(
        store.add(&ws, "i2", "乙", IdeaSource::Chat).is_err(),
        "不存在的工作空间不能记想法"
    );
}

/// 记想法时由存储生成不重复的 ID，调用方（Norma 适配器）不必自造。
///
/// 数据构造：同一工作空间连续 capture 同样正文两次。
/// 预期结果：
///   - 正断言：两条想法都进入清单且状态为待办。
///   - 负断言：两个 ID 不相等（相同正文不会被合并或覆盖）。
#[test]
fn capture_generates_distinct_ids_for_identical_bodies() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    let first = store
        .capture(&ws, "同一句话", IdeaSource::Voice)
        .expect("应记下第一条");
    let second = store
        .capture(&ws, "同一句话", IdeaSource::Voice)
        .expect("应记下第二条");
    assert_ne!(first.id, second.id, "相同正文也应生成不同 ID");
    assert_eq!(
        store.list(&ws).expect("应列出").len(),
        2,
        "两条想法都应在清单里"
    );
}
