use storage::{DesktopWorkspaceStore, WallpaperReference};

fn builtin(rel: &str) -> WallpaperReference {
    WallpaperReference::Builtin { rel: rel.into() }
}

/// WP-01：工作空间壁纸独立持久化，新空间复制来源选择后不再跟随来源。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间 = Alpha + Beta = 2；Alpha 初始 aurora，Beta 复制 aurora。
///   修改 Alpha 为 nebula 后，两空间有效选择数 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alpha 并把它的壁纸作为 Beta 初始选择。
///   2. 修改 Alpha，使用新 Store 模拟重启读取。
///
/// 预期结果：
///   - 正断言：Alpha 恢复 nebula，Beta 保留 aurora。
///   - 负断言：Beta 不能随 Alpha 改变。
#[test]
fn workspace_wallpapers_persist_independently_after_copy() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let aurora = builtin("wallpapers/aurora.webp");
    let nebula = builtin("wallpapers/nebula.webp");
    let alpha = store
        .create_with_wallpaper("Alpha", &aurora)
        .expect("应创建 Alpha");
    let beta = store
        .create_with_wallpaper("Beta", &aurora)
        .expect("应复制初始壁纸");
    store
        .set_workspace_wallpaper(&alpha.id, &nebula)
        .expect("应只更新 Alpha");
    let reopened = DesktopWorkspaceStore::new(path);
    let saved_alpha = reopened
        .get(&alpha.id)
        .expect("应读取 Alpha")
        .expect("Alpha 应存在");
    let saved_beta = reopened
        .get(&beta.id)
        .expect("应读取 Beta")
        .expect("Beta 应存在");
    assert_eq!(saved_alpha.wallpaper, Some(nebula), "Alpha 应恢复新壁纸");
    assert_eq!(
        saved_beta.wallpaper,
        Some(aurora),
        "Beta 应保留创建时复制的壁纸"
    );
    assert_ne!(
        saved_alpha.wallpaper, saved_beta.wallpaper,
        "复制不是持续跟随"
    );
}

/// WP-02：同项目的两个工作空间区域分别覆盖，清除覆盖不改变空间壁纸。
///
/// 数据构造：项目目录 = 1；工作空间 = 2；独立区域 = 1 + 1 = 2。
/// 执行过程：两空间添加同项目 → 仅甲区域覆盖 → 验证归属拒绝 → 重开 → 清除覆盖。
/// 预期结果：甲曾有覆盖，乙始终为空；跨空间写入失败，清除后空间选择保留。
#[test]
fn project_override_is_owned_by_workspace_area() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let background = builtin("wallpapers/aurora.webp");
    let alpha = store
        .create_with_wallpaper("Alpha", &background)
        .expect("应创建甲");
    let beta = store
        .create_with_wallpaper("Beta", &background)
        .expect("应创建乙");
    let first = store
        .add_project(&alpha.id, &project)
        .expect("应添加甲项目");
    let second = store.add_project(&beta.id, &project).expect("应添加乙项目");
    let override_value = builtin("wallpapers/nebula.webp");
    store
        .set_project_wallpaper(&alpha.id, &first.id, Some(&override_value))
        .expect("应保存甲覆盖");
    assert!(
        store
            .set_project_wallpaper(&beta.id, &first.id, Some(&background))
            .is_err(),
        "乙不能写入甲区域"
    );
    let reopened = DesktopWorkspaceStore::new(path);
    assert_eq!(
        reopened.list_projects(&alpha.id).expect("应读甲")[0].wallpaper,
        Some(override_value),
        "甲应恢复覆盖"
    );
    assert_eq!(
        reopened.list_projects(&beta.id).expect("应读乙")[0].wallpaper,
        None,
        "乙不能看到甲覆盖"
    );
    assert_ne!(first.id, second.id, "同项目两处必须有不同区域身份");
    reopened
        .set_project_wallpaper(&alpha.id, &first.id, None)
        .expect("应清除覆盖");
    assert_eq!(
        reopened.list_projects(&alpha.id).expect("应重新读甲")[0].wallpaper,
        None,
        "清除后应跟随"
    );
    assert_eq!(
        reopened
            .get(&alpha.id)
            .expect("应读甲空间")
            .expect("应存在")
            .wallpaper,
        Some(background),
        "清除覆盖不能改工作空间壁纸"
    );
}

/// WP-03：旧全局壁纸只回填一次，迁移和重启不能覆盖独立选择。
///
/// 数据构造：旧数据库含工作空间 1、全局选择 2 行；迁移后修改独立选择 1 次。
/// 执行过程：构造旧表 → 首次打开触发迁移 → 改独立选择及旧全局值 → 再次打开。
/// 预期结果：首次保留旧壁纸，重开保留新独立值，不跟随旧全局键变化。
#[test]
fn legacy_wallpaper_migration_is_idempotent() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let connection = rusqlite::Connection::open(&path).expect("应创建旧数据库");
    connection.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at_unix INTEGER NOT NULL);
        INSERT INTO settings VALUES('wallpaper.kind','custom',0),('wallpaper.value','old.png',0);
        CREATE TABLE desktop_workspaces(id TEXT PRIMARY KEY,name TEXT NOT NULL,created_at_unix_ms INTEGER NOT NULL,last_opened_at_unix_ms INTEGER NOT NULL);
        INSERT INTO desktop_workspaces VALUES('alpha','Alpha',0,0);").expect("应构造旧存档");
    drop(connection);
    let store = DesktopWorkspaceStore::new(path.clone());
    assert_eq!(
        store
            .get("alpha")
            .expect("应迁移")
            .expect("甲应存在")
            .wallpaper,
        Some(WallpaperReference::Custom {
            filename: "old.png".into()
        }),
        "升级应保留旧选择"
    );
    let changed = builtin("wallpapers/nebula.webp");
    store
        .set_workspace_wallpaper("alpha", &changed)
        .expect("应设置独立值");
    storage::SettingsStore::new(path.clone())
        .set_string("wallpaper.value", "other.png")
        .expect("应修改旧全局值");
    let reloaded = DesktopWorkspaceStore::new(path)
        .get("alpha")
        .expect("应重开")
        .expect("应存在");
    assert_eq!(reloaded.wallpaper, Some(changed), "重复迁移必须保留独立值");
    assert_ne!(
        reloaded.wallpaper,
        Some(WallpaperReference::Custom {
            filename: "other.png".into()
        }),
        "不得重新复制全局值"
    );
}

/// WP-04：移除共用素材前，空间恢复默认、项目清除覆盖，其他素材不受影响。
///
/// 数据构造：两空间、同项目两区域；共享 mine.png 被空间甲及区域乙引用，共 2 处。
/// 执行过程：查询受影响名称 → 清理引用 → 重开并读取四个选择。
/// 预期结果：准确列出甲与乙项目，甲恢复默认，乙区域跟随；乙空间与甲区域保留。
#[test]
fn shared_asset_cleanup_only_resets_referencing_scopes() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let custom = WallpaperReference::Custom {
        filename: "mine.png".into(),
    };
    let other = builtin("wallpapers/nebula.webp");
    let default = builtin("wallpapers/default.webp");
    let alpha = store
        .create_with_wallpaper("Alpha", &custom)
        .expect("应创建甲");
    let beta = store
        .create_with_wallpaper("Beta", &other)
        .expect("应创建乙");
    let first = store
        .add_project(&alpha.id, &project)
        .expect("应添加甲区域");
    let second = store.add_project(&beta.id, &project).expect("应添加乙区域");
    store
        .set_project_wallpaper(&alpha.id, &first.id, Some(&other))
        .expect("应设甲覆盖");
    store
        .set_project_wallpaper(&beta.id, &second.id, Some(&custom))
        .expect("应设乙覆盖");
    let usage = store.wallpaper_usage("mine.png").expect("应查询引用");
    assert_eq!(usage.workspaces, vec!["Alpha"], "只应列出甲空间");
    assert_eq!(usage.projects, vec!["Beta / project"], "只应列出乙项目");
    store
        .clear_custom_wallpaper("mine.png", &default)
        .expect("应清理引用");
    assert_eq!(
        store
            .get(&alpha.id)
            .expect("应读甲")
            .expect("应存在")
            .wallpaper,
        Some(default),
        "甲应恢复默认"
    );
    assert_eq!(
        store
            .get(&beta.id)
            .expect("应读乙")
            .expect("应存在")
            .wallpaper,
        Some(other.clone()),
        "乙空间应保留"
    );
    assert_eq!(
        store.list_projects(&alpha.id).expect("应读甲区域")[0].wallpaper,
        Some(other),
        "其他项目覆盖应保留"
    );
    assert_eq!(
        store.list_projects(&beta.id).expect("应读乙区域")[0].wallpaper,
        None,
        "乙项目应清除覆盖"
    );
    assert_eq!(
        store.wallpaper_usage("mine.png").expect("应查剩余引用"),
        storage::WallpaperUsage::default(),
        "被删素材不得留下活动引用"
    );
}
