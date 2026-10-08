use storage::{DatabasePaths, DesktopWorkspaceStore, ProjectStore};

/// DWS-01：两个空桌面工作空间具有独立身份，改名和固定创建顺序在重新打开数据库后仍保留。
///
/// 数据构造（含关键数值的推导过程）：
///   初始工作空间数 = 0；创建 Alpha 和 Beta 后 = 0 + 2 = 2。
///   两个空间都没有项目区域，因此两个 project_count 都 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alpha 和 Beta → 得到两个不同 ID，列表含两条空工作空间。
///   2. 将 Alpha 改名为 Design，再次打开 Beta → Beta 的最近打开时间更新，但不移到首位。
///   3. 用同一数据库路径创建新的 Store 并加载列表 → 名称、身份和顺序保持。
///
/// 预期结果：
///   - 正断言：Design 和 Beta 都存在且 ID 不同，Design 排在首位。
///   - 负断言：旧名称 Alpha 不再出现，两个空间都没有自动导入旧项目。
#[test]
fn desktop_workspace_catalog_persists_independent_empty_workspaces() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());

    let alpha = store.create("Alpha").expect("应创建 Alpha");
    let beta = store.create("Beta").expect("应创建 Beta");
    store.rename(&alpha.id, "Design").expect("应改名 Alpha");
    store.touch(&beta.id).expect("应更新 Beta 最近打开时间");

    let reloaded = DesktopWorkspaceStore::new(path)
        .list()
        .expect("重启后应重新读取工作空间");
    assert_eq!(reloaded.len(), 2, "两个空工作空间必须都持久化");
    assert_eq!(reloaded[0].id, alpha.id, "先创建的 Alpha 应固定在首位");
    assert_ne!(reloaded[0].id, beta.id, "最近打开的 Beta 不得移到首位");
    assert_eq!(reloaded[0].name, "Design", "Alpha 的新名称应持久化");
    assert_eq!(reloaded[1].id, beta.id, "Beta 身份应保持独立");
    assert_ne!(alpha.id, beta.id, "两个工作空间不得共享身份");
    assert_eq!(reloaded[0].project_count, 0, "新建 Design 应为空");
    assert_eq!(reloaded[1].project_count, 0, "新建 Beta 应为空");
    assert!(
        !reloaded.iter().any(|workspace| workspace.name == "Alpha"),
        "改名后旧名称不得残留"
    );
}

/// DWS-04：展开视角随工作空间保存，移除区域时清除失效视角。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间数 = 1，项目区域数 = 1；展开后记录 area ID，移除后剩余区域数 = 1 - 1 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建工作空间和项目区域，保存该区域的展开视角。
///   2. 重新创建 Store 并读取视角，然后移除区域。
///   3. 再次读取工作空间并尝试恢复已删除区域。
///
/// 预期结果：
///   - 正断言：重开 Store 后仍指向原区域，移除后恢复总览。
///   - 负断言：不能将不属于该工作空间的区域设为展开视角。
#[test]
fn desktop_expanded_area_survives_reopen_and_clears_on_remove() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let store = DesktopWorkspaceStore::new(db.clone());
    let workspace = store.create("Alpha").expect("应创建工作空间");
    let area = store
        .add_project(&workspace.id, &project)
        .expect("应创建区域");

    store
        .set_expanded_area(&workspace.id, Some(&area.id))
        .expect("应保存展开区域");
    let reopened = DesktopWorkspaceStore::new(db);
    assert_eq!(
        reopened
            .get(&workspace.id)
            .expect("应读取工作空间")
            .expect("工作空间应存在")
            .expanded_area_id
            .as_deref(),
        Some(area.id.as_str()),
        "重启后应恢复展开区域"
    );
    assert!(
        reopened
            .set_expanded_area(&workspace.id, Some("other-area"))
            .is_err(),
        "不能展开其他工作空间或不存在的区域"
    );
    reopened
        .remove_project(&workspace.id, &area.id)
        .expect("应移除区域");
    assert_eq!(
        reopened
            .get(&workspace.id)
            .expect("应读取工作空间")
            .expect("工作空间应存在")
            .expanded_area_id,
        None,
        "移除区域后应恢复总览"
    );
    assert!(
        reopened
            .list_projects(&workspace.id)
            .expect("应读取区域")
            .is_empty(),
        "移除后不得保留区域记录"
    );
}

/// DWS-04：总览滚动位置随工作空间分别持久化，切回时恢复原视口。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间数 = 2；甲滚动 -180 px，乙滚动 -40 px；位置差 = 140 px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建甲乙并分别保存总览纵向偏移。
///   2. 重新打开同一数据库并读取两条工作空间记录。
///   3. 核对两个偏移互不覆盖。
///
/// 预期结果：
///   - 正断言：甲为 -180，乙为 -40。
///   - 负断言：甲不能误恢复为乙的位置，乙不能误恢复为默认零点。
#[test]
fn desktop_overview_scroll_positions_are_independent_and_persistent() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let alpha = store.create("Alpha").expect("应创建甲");
    let beta = store.create("Beta").expect("应创建乙");
    store
        .set_overview_scroll_y(&alpha.id, -180)
        .expect("应保存甲位置");
    store
        .set_overview_scroll_y(&beta.id, -40)
        .expect("应保存乙位置");

    let reloaded = DesktopWorkspaceStore::new(path);
    let alpha_y = reloaded
        .get(&alpha.id)
        .expect("应读取甲")
        .expect("甲应存在")
        .overview_scroll_y;
    let beta_y = reloaded
        .get(&beta.id)
        .expect("应读取乙")
        .expect("乙应存在")
        .overview_scroll_y;
    assert_eq!(alpha_y, -180, "甲应恢复自己的总览位置");
    assert_eq!(beta_y, -40, "乙应恢复自己的总览位置");
    assert_ne!(alpha_y, beta_y, "两张工作空间不能共用总览位置");
    assert_ne!(beta_y, 0, "乙不能误恢复为默认零点");
}

/// DWS-01：首页缩略图使用已保存卡片的相对位置绘制项目画布快照。
///
/// 数据构造（含关键数值的推导过程）：
///   项目数 = 1；终端卡片 2 张，坐标分别为 (0,0)、(100,100)。
///   归一化后第一张 = (5,10)，第二张 = (85,80)，卡片数 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建桌面区域，在独立画布存档写入两个位置不同的节点。
///   2. 从工作空间目录加载首页预览。
///   3. 核对预览的项目身份、数量和位置。
///
/// 预期结果：
///   - 正断言：两张卡片按原相对位置出现，预览关联原区域。
///   - 负断言：两张卡片不能被压成同一位置，也不能错误标成 Agent。
#[test]
fn desktop_preview_preserves_saved_canvas_card_positions() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Alpha").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project)
        .expect("应添加项目");
    let canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &project, &workspace.id, &area.id)
            .expect("应初始化桌面画布");
    let mut document = canvas::Document::new();
    document.add_node(
        canvas::CanvasNodeKind::Term(canvas::TermSpec),
        canvas::CanvasRect::new(0.0, 0.0, 40.0, 30.0),
    );
    document.add_node(
        canvas::CanvasNodeKind::Term(canvas::TermSpec),
        canvas::CanvasRect::new(100.0, 100.0, 40.0, 30.0),
    );
    canvas
        .canvas_store()
        .save_snapshot(&document.to_snapshot())
        .expect("应保存两张卡片");

    let previews = catalog.list_previews().expect("应加载首页预览");
    let areas = previews.get(&workspace.id).expect("应有甲工作空间预览");
    assert_eq!(areas.len(), 1, "甲应只有一个项目分区");
    assert_eq!(areas[0].id, area.id, "预览应关联原区域身份");
    assert_eq!(areas[0].nodes.len(), 2, "预览应包含两张真实卡片节点");
    assert!(
        matches!(areas[0].nodes[0].kind, canvas::CanvasNodeKind::Term(_)),
        "第一张缩略卡片应保留真实 Terminal 类型"
    );
    assert!(
        matches!(areas[0].nodes[1].kind, canvas::CanvasNodeKind::Term(_)),
        "第二张缩略卡片应保留真实 Terminal 类型"
    );
    assert_ne!(
        areas[0].nodes[0].bounds.origin, areas[0].nodes[1].bounds.origin,
        "不同位置的卡片不可重叠成一点"
    );
    assert_eq!(
        areas[0].nodes[0].bounds,
        canvas::CanvasRect::new(0.0, 0.0, 40.0, 30.0),
        "缩略图必须保留第一张卡片的真实几何"
    );
    assert_eq!(
        areas[0].nodes[1].bounds,
        canvas::CanvasRect::new(100.0, 100.0, 40.0, 30.0),
        "缩略图必须保留第二张卡片的真实几何"
    );
    assert_ne!(
        areas[0].nodes[0].bounds, areas[0].nodes[1].bounds,
        "不同位置的卡片不得退化成同一通用矩形"
    );
}

/// DWS-01：空名称不能创建工作空间，也不能破坏已有工作空间的名称。
///
/// 数据构造（含关键数值的推导过程）：
///   初始工作空间数 = 0；非法创建后仍为 0；创建 Valid 后为 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用仅空格的名称创建 → 拒绝。
///   2. 创建 Valid，再用仅空格的名称改名 → 拒绝。
///   3. 重新读取数据库 → 仍只有 Valid。
///
/// 预期结果：
///   - 正断言：Valid 仍存在且名称未变。
///   - 负断言：非法名称不会生成额外工作空间。
#[test]
fn desktop_workspace_rejects_blank_names_without_writing() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));

    assert!(store.create("  ").is_err(), "空白名称不得创建工作空间");
    let valid = store.create("Valid").expect("应创建合法工作空间");
    assert!(
        store.rename(&valid.id, "\n ").is_err(),
        "空白名称不得覆盖已有名称"
    );

    let workspaces = store.list().expect("应重新读取目录");
    assert_eq!(workspaces.len(), 1, "非法操作不得新增记录");
    assert_eq!(workspaces[0].id, valid.id, "合法工作空间身份应保留");
    assert_eq!(workspaces[0].name, "Valid", "非法改名不得覆盖原名称");
    assert!(
        !workspaces
            .iter()
            .any(|workspace| workspace.name.trim().is_empty()),
        "目录中不应存在空白名称"
    );
}

/// DWS-05：同一项目在两张桌面工作空间内使用独立的新画布数据库，不读取旧项目画布。
///
/// 数据构造（含关键数值的推导过程）：
///   项目目录数 = 1；桌面工作空间数 = 2；预期新画布数据库数 = 2。
///   甲写入空快照一条；乙与旧项目存档均保持零条。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在同一项目目录下创建旧版 project.test.sqlite 哨兵快照。
///   2. 甲、乙分别初始化桌面区域的 ProjectStore。
///   3. 只向甲保存空画布快照，再读取甲、乙及旧版数据库。
///
/// 预期结果：
///   - 正断言：甲与乙使用不同路径，甲可恢复自己写入的快照。
///   - 负断言：乙不能看到甲的快照；新路径不等于旧项目存档路径。
#[test]
fn desktop_area_canvas_files_are_isolated_from_each_other_and_legacy_project() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("测试应创建项目目录");
    let paths = DatabasePaths::isolated_test(temp.path().join("data").as_path());
    let legacy =
        ProjectStore::initialize_for_database_paths(&paths, &project).expect("应初始化旧画布");
    let alpha = ProjectStore::initialize_for_desktop_area(&paths, &project, "alpha", "area-a")
        .expect("应初始化甲画布");
    let beta = ProjectStore::initialize_for_desktop_area(&paths, &project, "beta", "area-b")
        .expect("应初始化乙画布");
    let snapshot = canvas::Document::new().to_snapshot();
    alpha
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("甲应保存画布");

    assert_ne!(
        alpha.path(),
        beta.path(),
        "甲乙区域必须有不同画布数据库路径"
    );
    assert_ne!(
        alpha.path(),
        legacy.path(),
        "新桌面画布不得使用旧项目数据库"
    );
    assert_eq!(
        alpha.canvas_store().load_snapshot().expect("应读取甲画布"),
        Some(snapshot),
        "甲应恢复自己的快照"
    );
    assert!(
        beta.canvas_store()
            .load_snapshot()
            .expect("应读取乙画布")
            .is_none(),
        "乙不应读取甲的快照"
    );
    assert!(
        legacy
            .canvas_store()
            .load_snapshot()
            .expect("应读取旧画布")
            .is_none(),
        "新画布不应写入旧项目存档"
    );
}

/// DWS-08/DWS-09：删除区域或工作空间只清理其画布存档，不触碰同项目的其他空间和仓库。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间 = 2，两个空间各有项目 A 的 1 个区域；画布数据库 = 1 + 1 = 2。
///   删除甲/A 后画布数据库剩 1；再删除乙后剩 0；项目目录始终为 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 添加同项目到甲乙并分别初始化画布数据库。
///   2. 删除甲区域，重新读取甲乙目录和两张数据库路径。
///   3. 删除乙工作空间，检查乙目录和项目文件。
///
/// 预期结果：
///   - 正断言：乙区域与画布在删除甲后保留，最终项目目录仍存在。
///   - 负断言：甲画布及乙删除后的画布文件均不存在，甲区域和乙工作空间不残留。
#[test]
fn deleting_desktop_area_and_workspace_only_removes_owned_canvas_files() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("测试应创建项目目录");
    let paths = DatabasePaths::isolated_test(temp.path().join("data").as_path());
    let store = DesktopWorkspaceStore::new(paths.app_db_path().to_path_buf());
    let alpha = store.create("Alpha").expect("应创建甲");
    let beta = store.create("Beta").expect("应创建乙");
    let first = store
        .add_project(&alpha.id, &project)
        .expect("甲应添加项目");
    let second = store.add_project(&beta.id, &project).expect("乙应添加项目");
    let first_canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &project, &alpha.id, &first.id)
            .expect("应初始化甲画布");
    let second_canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &project, &beta.id, &second.id)
            .expect("应初始化乙画布");
    let first_path = first_canvas.path().to_path_buf();
    let second_path = second_canvas.path().to_path_buf();
    assert!(first_path.exists(), "甲画布应在删除前存在");
    assert!(second_path.exists(), "乙画布应在删除前存在");

    assert!(
        store
            .remove_project(&alpha.id, &first.id)
            .expect("应移除甲区域"),
        "甲区域应被删除"
    );
    assert!(!first_path.exists(), "甲画布数据库必须清理");
    assert!(second_path.exists(), "乙画布数据库必须保留");
    assert!(
        store
            .list_projects(&alpha.id)
            .expect("应读取甲区域")
            .is_empty(),
        "甲区域不得残留"
    );
    assert_eq!(
        store.list_projects(&beta.id).expect("应读取乙区域").len(),
        1,
        "乙区域必须保留"
    );

    assert!(
        store.delete(&beta.id).expect("应删除乙"),
        "乙工作空间应被删除"
    );
    assert!(!second_path.exists(), "乙画布数据库必须清理");
    assert!(
        store.get(&beta.id).expect("应读取乙").is_none(),
        "乙工作空间不得残留"
    );
    assert!(project.is_dir(), "项目目录必须保留");
}
