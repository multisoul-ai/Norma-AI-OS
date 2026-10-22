use storage::{DesktopWorkspaceStore, IdeaSource, IdeasStore};

fn setup(count: usize) -> (tempfile::TempDir, IdeasStore, String) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("空间")
        .expect("应创建工作空间");
    let store = IdeasStore::new(db);
    for index in 0..count {
        store
            .add(
                &workspace.id,
                &format!("i{index}"),
                &format!("想法{index}"),
                IdeaSource::Manual,
            )
            .expect("应记下想法");
    }
    (temp, store, workspace.id)
}

fn order(store: &IdeasStore, workspace: &str) -> Vec<String> {
    store
        .list(workspace)
        .expect("应列出")
        .into_iter()
        .map(|idea| idea.id)
        .collect()
}

/// 把想法移到另一条之前或末尾后，顺序立即生效并在重建 Store 后保留。
///
/// 数据构造：记下 i0..i3 四条待办，位置依次为 1024、2048、3072、4096。
/// 执行过程：i3 移到 i1 之前；再把 i0 移到末尾。
/// 预期结果：
///   - 正断言：顺序依次为 [i3, i1, i2, i0]；重建 Store 后相同。
///   - 负断言：移动不改变想法数量，不产生重复 ID。
#[test]
fn move_before_reorders_and_persists() {
    let (temp, store, ws) = setup(4);
    assert!(
        store
            .move_before(&ws, "i3", Some("i1"))
            .expect("移动不应出错"),
        "i3 应移到 i1 之前"
    );
    assert_eq!(
        order(&store, &ws),
        ["i0", "i3", "i1", "i2"],
        "第一次移动后的顺序"
    );
    assert!(
        store.move_before(&ws, "i0", None).expect("移动不应出错"),
        "i0 应移到末尾"
    );
    assert_eq!(
        order(&store, &ws),
        ["i3", "i1", "i2", "i0"],
        "第二次移动后的顺序"
    );
    let reopened = IdeasStore::new(temp.path().join("app.sqlite"));
    assert_eq!(
        order(&reopened, &ws),
        ["i3", "i1", "i2", "i0"],
        "重建 Store 后顺序应保留"
    );
}

/// 反复在同一处插入使位置间隙耗尽时，整体重排仍得到正确顺序。
///
/// 数据构造：三条待办 i0..i2。
/// 执行过程：连续 12 次把新的末尾项移到 i0 与 i1 之间的同一缝隙（每次取中点，约 10 次后间隙为 1）。
/// 预期结果：每次移动后相对顺序都符合预期；最终三条仍各出现一次。
#[test]
fn move_before_survives_gap_exhaustion() {
    let (_temp, store, ws) = setup(3);
    for round in 0..12 {
        // 轮流把 i2 与 i0 放到 i1 之前，位置间隙每次约减半。
        let moving = if round % 2 == 0 { "i2" } else { "i0" };
        assert!(
            store
                .move_before(&ws, moving, Some("i1"))
                .expect("移动不应出错"),
            "第 {round} 次应移动成功"
        );
        let ids = order(&store, &ws);
        let moved = ids.iter().position(|id| id == moving).expect("应在清单中");
        let anchor = ids.iter().position(|id| id == "i1").expect("应在清单中");
        assert_eq!(
            moved + 1,
            anchor,
            "第 {round} 次后 {moving} 应紧邻 i1 之前：{ids:?}"
        );
    }
    let mut ids = order(&store, &ws);
    ids.sort();
    assert_eq!(ids, ["i0", "i1", "i2"], "三条想法各出现一次");
}

/// 只有待办可参与排序：已指派、已删除、不存在的 ID 与自我引用都不改变顺序。
///
/// 数据构造：i0..i2 待办；i1 进入派发中；i2 被删除。
/// 执行过程：依次尝试移动 i1（派发中）、把 i0 移到 i2（已删除）之前、移动不存在的 ID、i0 移到自身之前。
/// 预期结果：全部返回 false，顺序保持 [i0, i1]。
#[test]
fn move_before_rejects_non_todo_and_invalid_targets() {
    let (_temp, store, ws) = setup(3);
    store
        .begin_dispatch(&ws, "i1", "ref-1")
        .expect("应进入派发中");
    store.dismiss(&ws, "i2").expect("应删除");
    assert!(
        !store.move_before(&ws, "i1", None).expect("不应报错"),
        "派发中的想法不可排序"
    );
    assert!(
        !store.move_before(&ws, "i0", Some("i2")).expect("不应报错"),
        "不能移到已删除想法之前"
    );
    assert!(
        !store.move_before(&ws, "ghost", None).expect("不应报错"),
        "不存在的想法返回 false"
    );
    assert!(
        !store.move_before(&ws, "i0", Some("i0")).expect("不应报错"),
        "不能移到自身之前"
    );
    assert_eq!(order(&store, &ws), ["i0", "i1"], "顺序不应变化");
}
