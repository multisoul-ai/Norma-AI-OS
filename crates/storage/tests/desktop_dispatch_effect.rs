use storage::{DesktopWorkspaceStore, DispatchEffectPreference};

fn pref(style: &str) -> DispatchEffectPreference {
    DispatchEffectPreference {
        style: style.into(),
        voice: None,
        idea: None,
        handoff: None,
    }
}

/// DE-01：工作空间与项目的派发特效各自持久化，空值表示跟随。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间 1 个（Alpha），项目 1 个；初始都没有特效设置 = 0 条。
///   工作空间写 ribbon + 交接覆盖 ion = 1 条；项目写 fireworks = 1 条。
///
/// 执行过程：
///   1. 读取初始值。
///   2. 分别写入工作空间与项目并用新连接读回。
///   3. 项目写入 None 取消覆盖。
///
/// 预期结果：
///   - 正断言：初始为 None；写入后各自读回自己的值，互不影响。
///   - 负断言：项目写 None 后回到跟随，工作空间的值不变。
#[test]
fn workspace_and_project_effects_persist_independently() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let workspace = store.create("Alpha").expect("应创建工作空间");
    let area = store
        .add_project(&workspace.id, &project)
        .expect("应添加项目");

    assert_eq!(
        store.workspace_dispatch_effect(&workspace.id).unwrap(),
        None,
        "新工作空间没有特效设置"
    );
    assert_eq!(
        store
            .project_dispatch_effect(&workspace.id, &area.id)
            .unwrap(),
        None,
        "新项目没有特效覆盖"
    );

    let mut ws = pref("ribbon");
    ws.handoff = Some("ion".into());
    store
        .set_workspace_dispatch_effect(&workspace.id, &ws)
        .expect("应保存工作空间特效");
    store
        .set_project_dispatch_effect(&workspace.id, &area.id, Some(&pref("fireworks")))
        .expect("应保存项目特效");

    let reopened = DesktopWorkspaceStore::new(path);
    assert_eq!(
        reopened.workspace_dispatch_effect(&workspace.id).unwrap(),
        Some(ws.clone()),
        "工作空间的值应原样读回"
    );
    assert_eq!(
        reopened
            .project_dispatch_effect(&workspace.id, &area.id)
            .unwrap(),
        Some(pref("fireworks")),
        "项目的值应原样读回"
    );

    reopened
        .set_project_dispatch_effect(&workspace.id, &area.id, None)
        .expect("应取消项目覆盖");
    assert_eq!(
        reopened
            .project_dispatch_effect(&workspace.id, &area.id)
            .unwrap(),
        None,
        "取消后回到跟随"
    );
    assert_eq!(
        reopened.workspace_dispatch_effect(&workspace.id).unwrap(),
        Some(ws),
        "取消项目覆盖不影响工作空间"
    );
}

/// DE-02：未知款式 id 原样保存（前向兼容旧版本），非法 id 被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   合法 id = 小写字母数字加下划线、长度 1..=32；"future_style" = 12 个字符，合法。
///   非法 id 4 种：空串、含空格、含 '/'、长度 33。
///
/// 执行过程：
///   1. 保存 future_style 并读回。
///   2. 逐个保存 4 种非法 id。
///   3. 在来源覆盖里放非法 id。
///
/// 预期结果：
///   - 正断言：未知但合法的 id 原样读回。
///   - 负断言：4 种非法 id 和非法来源覆盖都返回错误，且库里仍是上一次的值。
#[test]
fn unknown_style_is_kept_and_invalid_ids_are_rejected() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let workspace = store.create("Alpha").expect("应创建工作空间");

    store
        .set_workspace_dispatch_effect(&workspace.id, &pref("future_style"))
        .expect("未知但合法的 id 应保存");
    assert_eq!(
        store.workspace_dispatch_effect(&workspace.id).unwrap(),
        Some(pref("future_style")),
        "未知 id 应原样读回"
    );

    for bad in ["", "has space", "a/b", &"x".repeat(33)] {
        assert!(
            store
                .set_workspace_dispatch_effect(&workspace.id, &pref(bad))
                .is_err(),
            "非法 id {bad:?} 应被拒绝"
        );
    }
    let mut bad_source = pref("ribbon");
    bad_source.voice = Some("../etc".into());
    assert!(
        store
            .set_workspace_dispatch_effect(&workspace.id, &bad_source)
            .is_err(),
        "来源覆盖里的非法 id 应被拒绝"
    );
    assert_eq!(
        store.workspace_dispatch_effect(&workspace.id).unwrap(),
        Some(pref("future_style")),
        "拒绝后库里仍是上一次的值"
    );
}

/// DE-03：库里存了损坏的 JSON 时读成 None，不让设置页崩溃。
///
/// 数据构造（含关键数值的推导过程）：
///   直接用 SQL 把 dispatch_effect_json 写成 "{not json" = 1 条损坏数据。
///
/// 执行过程：
///   1. 创建工作空间并先触发迁移。
///   2. 用 SQL 写入损坏值后读取。
///
/// 预期结果：
///   - 正断言：读取返回 Ok(None)。
///   - 负断言：不返回 Err。
#[test]
fn corrupt_json_reads_as_follow() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let workspace = store.create("Alpha").expect("应创建工作空间");
    let connection = rusqlite::Connection::open(&path).expect("应打开数据库");
    connection
        .execute(
            "UPDATE desktop_workspaces SET dispatch_effect_json='{not json' WHERE id=?1",
            [&workspace.id],
        )
        .expect("应写入损坏值");
    assert_eq!(
        store.workspace_dispatch_effect(&workspace.id).unwrap(),
        None,
        "损坏数据应当作未设置"
    );
}

/// DE-04：批量读取一次拿到所有工作空间与项目的设置，跳过未设置和损坏的行。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间 2 个（Alpha 设置 ribbon，Beta 未设置）；项目 2 个（Alpha 下 fireworks，另一个损坏 JSON）。
///   期望：工作空间 1 条、项目 1 条。
///
/// 预期结果：
///   - 正断言：返回 Alpha 的 ribbon 与对应项目的 fireworks，带正确的 id。
///   - 负断言：未设置的 Beta 和 JSON 损坏的项目都不出现。
#[test]
fn list_returns_only_valid_rows() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir(&first).expect("应创建项目一");
    std::fs::create_dir(&second).expect("应创建项目二");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let alpha = store.create("Alpha").expect("应创建甲");
    let _beta = store.create("Beta").expect("应创建乙");
    let good = store.add_project(&alpha.id, &first).expect("应添加项目一");
    let bad = store.add_project(&alpha.id, &second).expect("应添加项目二");
    store
        .set_workspace_dispatch_effect(&alpha.id, &pref("ribbon"))
        .expect("应保存工作空间");
    store
        .set_project_dispatch_effect(&alpha.id, &good.id, Some(&pref("fireworks")))
        .expect("应保存项目");
    rusqlite::Connection::open(&path)
        .expect("应打开数据库")
        .execute(
            "UPDATE desktop_project_areas SET dispatch_effect_json='{bad' WHERE id=?1",
            [&bad.id],
        )
        .expect("应写入损坏值");

    let snapshot = store.list_dispatch_effects().expect("应读取批量设置");
    assert_eq!(
        snapshot.workspaces,
        vec![(alpha.id.clone(), pref("ribbon"))],
        "只应有已设置的工作空间"
    );
    assert_eq!(
        snapshot.projects,
        vec![(alpha.id.clone(), good.id.clone(), pref("fireworks"))],
        "只应有有效的项目覆盖"
    );
}

/// DE-05：清除工作空间的特效设置后回到“未设置”，批量读取也不再返回它。
///
/// 数据构造：工作空间 1 个，先写 ribbon 再清除；另一个工作空间 Beta 保持 ribbon 不动。
/// 预期结果：
///   - 正断言：清除后读取为 None；批量读取只剩 Beta。
///   - 负断言：清除不存在的工作空间返回错误，不影响其他行。
#[test]
fn clearing_a_workspace_returns_it_to_unset() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let alpha = store.create("Alpha").expect("应创建甲");
    let beta = store.create("Beta").expect("应创建乙");
    for id in [&alpha.id, &beta.id] {
        store
            .set_workspace_dispatch_effect(id, &pref("ribbon"))
            .expect("应保存");
    }
    store
        .clear_workspace_dispatch_effect(&alpha.id)
        .expect("应清除甲");
    assert_eq!(store.workspace_dispatch_effect(&alpha.id).unwrap(), None);
    assert_eq!(
        store.list_dispatch_effects().unwrap().workspaces,
        vec![(beta.id.clone(), pref("ribbon"))],
        "批量读取只应剩下乙"
    );
    assert!(
        store.clear_workspace_dispatch_effect("missing").is_err(),
        "不存在的工作空间应报错"
    );
}
