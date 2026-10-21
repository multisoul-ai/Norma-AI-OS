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

/// 任务阶段回写：成功→完成、取消→回待办，失败/运行中不动；幂等且只碰本工作空间的已指派想法。
///
/// 数据构造：本空间四条已指派想法，task_ref 分别为 t-ok、t-cancel、t-fail、t-run；另有一条待办和另一空间的已指派（ref t-ok2）。
/// 执行过程：用查找函数（t-ok→成功、t-cancel→取消、t-fail→失败、t-run→运行中、t-ok2→成功）调用两次 apply_task_phases。
/// 预期结果：
///   - 正断言：第一次处理 2 条（完成与退回）；t-ok 为 done，t-cancel 为 todo 且 task_ref 清空；t-fail、t-run 仍是已指派。
///   - 负断言：第二次处理 0 条（幂等）；待办未被改动；另一空间的想法不受影响。
#[test]
fn applying_task_phases_completes_or_retracts_idempotently() {
    use workspace::ideas::TaskPhase;
    let (_temp, db, ws) = setup();
    let other = DesktopWorkspaceStore::new(db.clone())
        .create("其他")
        .expect("应创建")
        .id;
    let store = IdeasStore::new(db);
    for (id, task_ref) in [
        ("a", "t-ok"),
        ("b", "t-cancel"),
        ("c", "t-fail"),
        ("d", "t-run"),
    ] {
        store.add(&ws, id, id, IdeaSource::Chat).expect("应记下");
        store
            .begin_dispatch(&ws, id, task_ref)
            .expect("不应出错")
            .expect("应开始派发");
        store
            .mark_assigned(&ws, id, "area", 1, None)
            .expect("不应出错")
            .expect("应指派");
    }
    store.add(&ws, "e", "e", IdeaSource::Chat).expect("应记下");
    store
        .add(&other, "x", "x", IdeaSource::Chat)
        .expect("应记下");
    store
        .begin_dispatch(&other, "x", "t-ok2")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .mark_assigned(&other, "x", "area", 1, None)
        .expect("不应出错")
        .expect("应指派");
    let lookup = |task_ref: &str| match task_ref {
        "t-ok" | "t-ok2" => Some(TaskPhase::Succeeded),
        "t-cancel" => Some(TaskPhase::Cancelled),
        "t-fail" => Some(TaskPhase::Failed),
        "t-run" => Some(TaskPhase::Running),
        _ => None,
    };
    assert_eq!(
        store.apply_task_phases(&ws, lookup).expect("回写不应出错"),
        2,
        "应完成一条并退回一条"
    );
    let by_id = |id: &str| {
        store
            .list(&ws)
            .expect("应列出")
            .into_iter()
            .find(|i| i.id == id)
            .expect("应存在")
    };
    assert_eq!(by_id("a").state, IdeaState::Done, "成功的任务应让想法完成");
    assert_eq!(
        (by_id("b").state, by_id("b").task_ref),
        (IdeaState::Todo, None),
        "取消的任务应让想法回待办并清除 task_ref"
    );
    assert_eq!(
        by_id("c").state,
        IdeaState::Assigned,
        "失败的任务应保持已指派，由界面提示重试"
    );
    assert_eq!(
        by_id("d").state,
        IdeaState::Assigned,
        "运行中的任务不应改变想法"
    );
    assert_eq!(by_id("e").state, IdeaState::Todo, "待办不应被回写改动");
    assert_eq!(
        store.apply_task_phases(&ws, lookup).expect("回写不应出错"),
        0,
        "重复回写应幂等"
    );
    assert_eq!(
        store.list(&other).expect("应列出")[0].state,
        IdeaState::Assigned,
        "其他空间的想法不应被本空间回写改动"
    );
}

/// 已完成的想法保留一段时间后清理；未到期的、待办与已指派不受影响。
///
/// 数据构造：一条完成、一条已指派、一条待办；cutoff 取“现在 − 1000 秒”（完成时间在其后）与“现在 + 1000 秒”（完成时间在其前）。
/// 预期结果：
///   - 正断言：cutoff 在未来时清理 1 条已完成。
///   - 负断言：cutoff 在过去时清理 0 条；已指派与待办始终保留。
#[test]
fn purge_removes_only_old_done_ideas() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    for id in ["a", "b", "c"] {
        store.add(&ws, id, id, IdeaSource::Chat).expect("应记下");
    }
    store
        .begin_dispatch(&ws, "a", "t-a")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .mark_assigned(&ws, "a", "area", 1, None)
        .expect("不应出错")
        .expect("应指派");
    assert!(
        store.complete_by_task_ref("t-a").expect("不应出错"),
        "a 应完成"
    );
    store
        .begin_dispatch(&ws, "b", "t-b")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .mark_assigned(&ws, "b", "area", 1, None)
        .expect("不应出错")
        .expect("应指派");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟应正常")
        .as_secs();
    assert_eq!(
        store
            .purge_done_before(&ws, now - 1_000)
            .expect("清理不应出错"),
        0,
        "未到期的已完成想法不应被清理"
    );
    assert_eq!(
        store
            .purge_done_before(&ws, now + 1_000)
            .expect("清理不应出错"),
        1,
        "到期的已完成想法应被清理"
    );
    let ids: Vec<String> = store
        .list(&ws)
        .expect("应列出")
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, ["b", "c"], "已指派与待办应保留");
}

/// 启动对账只处理停留足够久的“派发中”：刚开始的可能还在发，不能被抢先回退。
///
/// 数据构造：一条刚进入派发中的想法（年龄 0 秒）。
/// 预期结果：
///   - 负断言：最小年龄 3600 秒时返回 0，想法仍是派发中（不会被误判为没发出而回待办）。
///   - 正断言：最小年龄 0 且找不到真实任务时才回待办。
#[test]
fn reconcile_skips_fresh_dispatching_ideas() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    store.add(&ws, "a", "a", IdeaSource::Chat).expect("应记下");
    store
        .begin_dispatch(&ws, "a", "idea:a")
        .expect("不应出错")
        .expect("应开始派发");
    assert_eq!(
        store
            .reconcile_dispatching_with(&ws, 3_600, |_| None)
            .expect("不应出错"),
        0,
        "刚开始的派发不应被对账回退"
    );
    assert_eq!(
        store.list(&ws).expect("应列出")[0].state,
        IdeaState::Dispatching,
        "仍应是派发中"
    );
    assert_eq!(
        store
            .reconcile_dispatching_with(&ws, 0, |_| None)
            .expect("不应出错"),
        1,
        "年龄达标且没发出时才回待办"
    );
    assert_eq!(
        store.list(&ws).expect("应列出")[0].state,
        IdeaState::Todo,
        "应回到待办"
    );
}

/// 手机端只读视图：只暴露截断后的标题与粗粒度状态，不暴露正文、路径、任务引用，也不暴露已删除的想法。
///
/// 数据构造：一条多行想法（首行“登录页改版”）、一条 100 个字符的单行、一条派发中、一条已删除、再补 105 条待办凑超限。
/// 执行过程：调用 mobile_view。
/// 预期结果：
///   - 正断言：多行想法标题只含首行；100 字符标题截成 80 字符加省略号；派发中显示为 assigned；条数被限制为 100。
///   - 负断言：已删除的不出现；视图里没有正文第二行内容。
#[test]
fn mobile_view_is_bounded_coarse_and_hides_dismissed() {
    let (_temp, db, ws) = setup();
    let store = IdeasStore::new(db);
    store
        .add(&ws, "m1", "登录页改版\n第二行是内部细节", IdeaSource::Chat)
        .expect("应记下");
    store
        .add(&ws, "m2", &"长".repeat(100), IdeaSource::Chat)
        .expect("应记下");
    store
        .add(&ws, "m3", "派发中的事", IdeaSource::Chat)
        .expect("应记下");
    store
        .begin_dispatch(&ws, "m3", "idea:m3")
        .expect("不应出错")
        .expect("应开始派发");
    store
        .add(&ws, "m4", "删掉的事", IdeaSource::Chat)
        .expect("应记下");
    store.dismiss(&ws, "m4").expect("不应出错").expect("应删除");
    let view = store.mobile_view(&ws).expect("应生成手机视图");
    assert_eq!(view[0].title, "登录页改版", "多行想法只应暴露首行");
    assert_eq!(
        view[1].title,
        format!("{}…", "长".repeat(80)),
        "超过 80 字符应截断并加省略号"
    );
    assert_eq!(
        (view[2].title.as_str(), view[2].state),
        ("派发中的事", "assigned"),
        "派发中应显示为 assigned"
    );
    assert!(
        view.iter().all(|idea| idea.title != "删掉的事"),
        "已删除的想法不应出现"
    );
    assert!(
        view.iter().all(|idea| !idea.title.contains("内部细节")),
        "不应泄露正文的其余部分"
    );
    for n in 0..105 {
        store
            .add(&ws, &format!("pad{n}"), "占位", IdeaSource::Chat)
            .expect("应记下");
    }
    assert_eq!(
        store.mobile_view(&ws).expect("应生成手机视图").len(),
        100,
        "条数应限制为 100"
    );
}
