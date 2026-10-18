use storage::{
    DesktopContentKind, DesktopContentTabStore, DesktopWorkspaceStore, NewDesktopContentTab,
};

/// 同一工作空间的内容标签应按资源复用，并在重启后保留来源和选中项。
///
/// 数据构造：甲乙两个工作空间；甲含项目 A，一条网页资源在甲连续打开两次。
/// 执行过程：打开网页、再次打开同一 URL、重建 Store，并分别读取甲乙标签。
/// 预期结果：甲只有一条选中标签且来源项目正确；乙没有标签，项目画布不参与标签身份。
#[test]
fn browser_tab_is_reused_and_restored_with_workspace_scope() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let project = temp.path().join("project-a");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let workspaces = DesktopWorkspaceStore::new(db.clone());
    let alpha = workspaces.create("甲").expect("应创建甲工作空间");
    let beta = workspaces.create("乙").expect("应创建乙工作空间");
    let area = workspaces
        .add_project(&alpha.id, &project)
        .expect("应添加项目 A");
    let tabs = DesktopContentTabStore::new(db.clone());
    let request = NewDesktopContentTab {
        workspace_id: alpha.id.clone(),
        source_area_id: Some(area.id.clone()),
        source_project_name: area.display_name.clone(),
        kind: DesktopContentKind::Browser,
        resource: "https://example.test/docs".to_string(),
        version: None,
        title: "文档".to_string(),
    };

    let first = tabs.open_or_select(request.clone()).expect("应打开网页");
    let second = tabs.open_or_select(request).expect("应复用网页");
    assert_eq!(first.id, second.id, "重复打开同一网页应复用标签 ID");
    assert!(second.selected, "再次打开的标签应被选中");
    assert_eq!(
        second.source_area_id.as_deref(),
        Some(area.id.as_str()),
        "来源项目应保留"
    );

    let reopened = DesktopContentTabStore::new(db);
    let alpha_tabs = reopened.list(&alpha.id).expect("重启后应读取甲标签");
    let beta_tabs = reopened.list(&beta.id).expect("重启后应读取乙标签");
    assert_eq!(alpha_tabs.len(), 1, "甲连续打开两次仍只能有一条标签");
    assert_eq!(alpha_tabs[0].id, first.id, "重启后标签 ID 应稳定");
    assert!(alpha_tabs[0].selected, "重启后应恢复选中项");
    assert!(
        !alpha_tabs[0].source_removed,
        "仍在工作空间的项目不能标记为已移除"
    );
    assert!(beta_tabs.is_empty(), "乙不得看到甲的网页标签");
}

/// 网页标签内部跳转后，应保存新地址；导航到已有标签地址时保留当前标签身份。
///
/// 数据构造：同一空间内打开 A、B 两个网页标签，B 是当前标签。
/// 执行过程：B 先跳转 C，再跳转 A，随后重建 Store 读取持久状态。
/// 预期结果：当前标签 B 的 ID 和选中状态不变，资源变为 A；原 A 标签合并，不留下重复地址。
#[test]
fn browser_navigation_updates_selected_tab_and_merges_duplicate_location() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("网页空间")
        .expect("应创建工作空间");
    let tabs = DesktopContentTabStore::new(db.clone());
    let open = |resource: &str, title: &str| {
        tabs.open_or_select(NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "网页空间".into(),
            kind: DesktopContentKind::Browser,
            resource: resource.into(),
            version: None,
            title: title.into(),
        })
        .expect("应打开网页标签")
    };
    let first = open("https://a.example.test/", "A");
    let current = open("https://b.example.test/", "B");
    assert!(
        tabs.update_browser_location(&workspace.id, &current.id, "https://c.example.test/", "C")
            .expect("应保存第一次导航")
            .changed,
        "不同地址导航必须返回已更新"
    );
    assert!(
        tabs.update_browser_location(&workspace.id, &current.id, "https://a.example.test/", "A")
            .expect("应合并重复地址")
            .changed,
        "导航到已有地址也必须更新当前标签"
    );
    let saved = DesktopContentTabStore::new(db)
        .list(&workspace.id)
        .expect("应恢复网页标签");
    assert_eq!(saved.len(), 1, "相同网页地址不得留下两个标签");
    assert_eq!(saved[0].id, current.id, "导航中的标签身份应保留");
    assert_ne!(saved[0].id, first.id, "旧的重复标签应被合并");
    assert_eq!(
        saved[0].resource, "https://a.example.test/",
        "应保存最终地址"
    );
    assert_eq!(saved[0].title, "A", "标签标题应跟随新地址");
    assert!(saved[0].selected, "导航中的标签应保持选中");
}

/// 隐藏网页的迟到导航不能删除当前选中的同地址标签。
///
/// 数据构造：先打开 B，再打开并选中 A；B 的原生网页仍可能上报迟到跳转。
/// 执行过程：让后台 B 导航到 A 已占用的地址。
/// 预期结果：保留 A 和 selected=1，合并掉后台 B；右栏始终有选中内容。
#[test]
fn background_browser_navigation_preserves_selected_duplicate_tab() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("后台导航空间")
        .expect("应创建工作空间");
    let tabs = DesktopContentTabStore::new(db.clone());
    let open = |resource: &str| {
        tabs.open_or_select(NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "后台导航空间".into(),
            kind: DesktopContentKind::Browser,
            resource: resource.into(),
            version: None,
            title: resource.into(),
        })
        .expect("应打开网页标签")
    };
    let background = open("https://b.example.test/");
    let selected = open("https://a.example.test/");
    assert!(
        tabs.update_browser_location(
            &workspace.id,
            &background.id,
            "https://a.example.test/",
            "A",
        )
        .expect("迟到导航应被安全合并")
        .changed,
        "重复地址导航必须被处理"
    );
    let saved = DesktopContentTabStore::new(db)
        .list(&workspace.id)
        .expect("应读取最终标签");
    assert_eq!(saved.len(), 1, "重复地址只应保留一个标签");
    assert_eq!(saved[0].id, selected.id, "必须保留当前可见的 A 标签");
    assert_ne!(saved[0].id, background.id, "后台的 B 标签应被合并");
    assert!(saved[0].selected, "右栏必须继续有选中内容");
}

/// 不同文件版本应分开，关闭选中标签后自动选中剩余标签；移除来源项目不删除内容。
///
/// 数据构造：一个工作空间含项目 B，同一文件的 v1、v2 两个版本及一条网页标签。
/// 执行过程：依次打开三个标签，选中网页后关闭；移除项目 B 并重新读取标签。
/// 预期结果：保留两个不同版本，v2 成为选中项且都标明来源已移除；本地文件仍存在。
#[test]
fn file_versions_and_removed_project_tabs_survive_selection_changes() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let project = temp.path().join("project-b");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let file = project.join("plan.md");
    std::fs::write(&file, "# Plan").expect("应写入真实文件");
    let workspaces = DesktopWorkspaceStore::new(db.clone());
    let workspace = workspaces.create("空间").expect("应创建工作空间");
    let area = workspaces
        .add_project(&workspace.id, &project)
        .expect("应添加项目 B");
    let tabs = DesktopContentTabStore::new(db.clone());
    let request = |kind, version: Option<&str>, resource: String| NewDesktopContentTab {
        workspace_id: workspace.id.clone(),
        source_area_id: Some(area.id.clone()),
        source_project_name: area.display_name.clone(),
        kind,
        resource,
        version: version.map(str::to_owned),
        title: "方案".to_string(),
    };
    let file_path = file.to_string_lossy().into_owned();
    let v1 = tabs
        .open_or_select(request(
            DesktopContentKind::File,
            Some("v1"),
            file_path.clone(),
        ))
        .expect("应打开 v1");
    let v2 = tabs
        .open_or_select(request(DesktopContentKind::File, Some("v2"), file_path))
        .expect("应打开 v2");
    assert_ne!(v1.id, v2.id, "不同文件版本必须是不同标签");
    let browser = tabs
        .open_or_select(request(
            DesktopContentKind::Browser,
            None,
            "https://example.test".to_string(),
        ))
        .expect("应打开网页");
    tabs.select(&workspace.id, &browser.id).expect("应选中网页");
    assert!(
        tabs.close(&workspace.id, &browser.id).expect("应关闭网页"),
        "关闭已有标签应报告成功"
    );
    assert!(
        workspaces
            .remove_project(&workspace.id, &area.id)
            .expect("应移除项目"),
        "项目 B 应已移除"
    );

    let reopened = DesktopContentTabStore::new(db);
    let remaining = reopened.list(&workspace.id).expect("应恢复剩余标签");
    assert_eq!(remaining.len(), 2, "关闭网页后应保留两个文件版本");
    assert_eq!(remaining[0].id, v1.id, "v1 身份应保留");
    assert_eq!(remaining[1].id, v2.id, "v2 身份应保留");
    assert!(remaining[1].selected, "关闭选中网页后应选中最近的文件版本");
    assert!(!remaining[0].selected, "不能同时选中两个标签");
    assert!(
        remaining.iter().all(|tab| tab.source_removed),
        "来源项目移除后，两个文件标签都应标明来源已移除"
    );
    assert!(file.exists(), "移除项目和关闭标签均不得删除本地文件");
}
