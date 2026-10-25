use canvas::PreviewType;
use storage::{
    DesktopContentKind, DesktopContentTabStore, DesktopWorkspaceStore, NewDesktopContentTab,
};

/// 场景：Present 的 --new 创建独立标签，普通展示复用选中标签且保留显式类型。
/// 数据构造：隔离工作空间，同一无扩展名文件，先按 Markdown 展示。
/// 执行过程：新建、强制新建、按 Text 复用，重新打开 Store 读取。
/// 预期结果：两个身份不同的标签，仅最新标签选中，类型变更持久化，version 不被挪用。
#[test]
fn presentation_duplicates_and_explicit_types_survive_reopen() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("Present")
        .expect("工作空间");
    let store = DesktopContentTabStore::new(db.clone());
    let request = NewDesktopContentTab {
        workspace_id: workspace.id.clone(),
        source_area_id: None,
        source_project_name: "Project".into(),
        kind: DesktopContentKind::File,
        resource: "/tmp/presentation-README".into(),
        version: None,
        title: "README".into(),
    };
    let (first, created) = store
        .open_for_presentation(request.clone(), false, Some(PreviewType::Markdown))
        .expect("首次展示");
    assert!(created, "首次打开应报告 Created");
    let (second, created) = store
        .open_for_presentation(request.clone(), true, Some(PreviewType::Markdown))
        .expect("独立展示");
    assert!(created, "--new 应报告 Created");
    assert_ne!(first.id, second.id, "--new 不得复用已有标签");
    let (reused, created) = store
        .open_for_presentation(request, false, Some(PreviewType::Text))
        .expect("复用展示");
    assert!(!created, "普通展示应报告 Reused");
    assert_eq!(reused.id, second.id, "应复用当前选中资源");
    let tabs = DesktopContentTabStore::new(db)
        .list(&workspace.id)
        .expect("恢复标签");
    assert_eq!(tabs.len(), 2, "两个独立标签均应持久化");
    assert_eq!(
        tabs[1].preview_type,
        Some(PreviewType::Text),
        "显式 Text 类型必须恢复"
    );
    assert!(tabs[1].selected && !tabs[0].selected, "应仅选中新标签");
    assert!(
        tabs.iter().all(|tab| tab.version.is_none()),
        "标签身份与类型不能占用冻结版本字段"
    );
}

/// 场景：首次即 --new；副本网页导航不能吞掉同址的其他独立标签。
/// 数据构造：一个工作空间，先强制创建网页 A，再普通复用并创建第二个副本 B。
/// 执行过程：B 先跳到不同 URL，再导航回 A；从存储重新读取。
/// 预期结果：默认展示复用已有副本；导航后两个独立标签都在，只有 B 选中。
#[test]
fn first_copy_can_be_reused_and_navigation_keeps_independent_copies() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("Copies")
        .expect("工作空间");
    let store = DesktopContentTabStore::new(db);
    let request = NewDesktopContentTab {
        workspace_id: workspace.id.clone(),
        source_area_id: None,
        source_project_name: "Project".into(),
        kind: DesktopContentKind::Browser,
        resource: "https://example.com/".into(),
        version: None,
        title: "Example".into(),
    };
    let (first, _) = store
        .open_for_presentation(request.clone(), true, None)
        .expect("首次副本");
    let (reused, created) = store
        .open_for_presentation(request.clone(), false, None)
        .expect("默认复用");
    assert_eq!(reused.id, first.id, "没有普通标签时仍须复用副本");
    assert!(!created, "默认复用不能伪报 Created");
    let (second, _) = store
        .open_for_presentation(request, true, None)
        .expect("第二副本");
    store
        .update_browser_location(&workspace.id, &second.id, "https://other.example/", "Other")
        .expect("导航");
    let update = store
        .update_browser_location(&workspace.id, &second.id, "https://example.com/", "Example")
        .expect("返回同址");
    assert_eq!(update.removed_tab_id, None, "独立副本不得被地址去重删除");
    let tabs = store.list(&workspace.id).expect("读取标签");
    assert_eq!(tabs.len(), 2, "两个独立网页必须保留");
    assert!(tabs[1].selected && !tabs[0].selected, "选中状态必须稳定");
}

/// 场景：升级前的内容标签迁移不能丢失身份、冻结版本和选择状态。
/// 数据构造：先建立隔离工作空间，再换回旧表结构，写入位置 7 的已选冻结文件标签。
/// 执行过程：通过生产 Store.list 触发迁移，再按同资源创建 --new 标签。
/// 预期结果：旧标签所有展示信息保留，新列取默认值；新副本可独立保存。
#[test]
fn presentation_migration_preserves_existing_content_tabs() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("Legacy")
        .expect("工作空间");
    let connection = rusqlite::Connection::open(&db).expect("旧表夹具连接");
    connection
        .execute_batch(
            "DROP TABLE desktop_content_tabs;
        CREATE TABLE desktop_content_tabs (
            id TEXT PRIMARY KEY NOT NULL, workspace_id TEXT NOT NULL,
            source_area_id TEXT, source_project_name TEXT NOT NULL,
            kind TEXT NOT NULL, resource TEXT NOT NULL, version TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL, position INTEGER NOT NULL, selected INTEGER NOT NULL DEFAULT 0,
            UNIQUE(workspace_id,kind,resource,version));",
        )
        .expect("旧表结构");
    connection.execute("INSERT INTO desktop_content_tabs VALUES('legacy',?1,NULL,'Old Project','file','/tmp/readme.md','rev-A','Old Title',7,1)", [&workspace.id]).expect("旧标签");
    drop(connection);
    let store = DesktopContentTabStore::new(db);
    let tabs = store.list(&workspace.id).expect("迁移读取");
    assert_eq!(tabs.len(), 1, "迁移后不能丢标签");
    let old = &tabs[0];
    assert_eq!(old.id, "legacy", "迁移保留表面身份");
    assert_eq!(
        old.version.as_deref(),
        Some("rev-A"),
        "冻结版本不能被新身份字段覆盖"
    );
    assert_eq!(
        (
            old.title.as_str(),
            old.source_project_name.as_str(),
            old.position
        ),
        ("Old Title", "Old Project", 7),
        "来源、标题和顺序必须保留"
    );
    assert!(
        old.selected && old.instance_id.is_empty(),
        "旧选中状态保留，默认实例为空"
    );
    assert_eq!(old.preview_type, None, "旧类型不能凭空变成显式类型");
    let (copy, created) = store
        .open_for_presentation(
            NewDesktopContentTab {
                workspace_id: workspace.id,
                source_area_id: None,
                source_project_name: "Old Project".into(),
                kind: DesktopContentKind::File,
                resource: old.resource.clone(),
                version: old.version.clone(),
                title: old.title.clone(),
            },
            true,
            Some(PreviewType::Markdown),
        )
        .expect("迁移后创建副本");
    assert!(created, "新副本必须成功创建");
    assert_ne!(copy.id, old.id, "副本不得覆盖旧标签");
}
