use std::process::Command;
use storage::{DatabasePaths, DesktopWorkspaceStore, ProjectStore};

/// 同一空间重复添加相同项目，创建独立画布，移除其中一张不影响另一张。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个项目目录、1 个空间；两次添加后画布数 = 0 + 1 + 1 = 2。
///   首张保存空快照，第二张无快照；移除首张后画布数 = 2 - 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 同目录添加两次 → 创建不同 area ID，引用相同项目根目录和 key。
///   2. 分别初始化画布，只保存首张快照 → 第二张仍没有快照。
///   3. 重开目录 → 两张均保留；移除首张 → 只删除首张记录和画布文件。
///
/// 预期结果：
///   - 正断言：第二张画布记录、文件和原项目目录保留。
///   - 负断言：两张不共用 ID 或文件；首张记录和文件不再存在。
#[test]
fn repeated_project_additions_keep_canvases_independent() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = store.create("Workspace").expect("应创建空间");
    let first = store
        .add_project(&workspace.id, &project)
        .expect("首次添加应成功");
    let second = store
        .add_project(&workspace.id, &project)
        .expect("重复添加应成功");
    assert_ne!(first.id, second.id, "重复添加必须创建独立画布身份");
    assert_eq!(first.root, second.root, "两张画布应引用同一目录");
    assert_eq!(first.project_key, second.project_key, "项目身份应保持一致");
    let first_canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &project, &workspace.id, &first.id)
            .expect("应初始化首张画布");
    let second_canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &project, &workspace.id, &second.id)
            .expect("应初始化第二张画布");
    assert_ne!(
        first_canvas.path(),
        second_canvas.path(),
        "画布文件必须独立"
    );
    let snapshot = canvas::Document::new().to_snapshot();
    first_canvas
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("应保存首张快照");
    assert!(
        second_canvas
            .canvas_store()
            .load_snapshot()
            .expect("应读取第二张")
            .is_none(),
        "第二张不得继承首张的快照"
    );
    let reopened = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    assert_eq!(
        reopened
            .list_projects(&workspace.id)
            .expect("应重开目录")
            .len(),
        2,
        "重开后两张画布必须保留"
    );
    assert!(
        reopened
            .remove_project(&workspace.id, &first.id)
            .expect("应移除首张"),
        "首张记录应被删除"
    );
    let remaining = reopened
        .list_projects(&workspace.id)
        .expect("应读取剩余画布");
    assert_eq!(remaining.len(), 1, "仅应删除一张画布");
    assert_eq!(remaining[0].id, second.id, "第二张必须保留");
    assert!(
        !remaining.iter().any(|area| area.id == first.id),
        "首张记录不得残留"
    );
    assert!(!first_canvas.path().exists(), "首张画布文件必须清理");
    assert!(second_canvas.path().exists(), "第二张画布文件不得清理");
    assert!(project.exists(), "项目目录不得删除");
}

/// 同项目的画布可独立改名，名称校验和重新加载不改变项目身份。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个项目、2 个空间；甲的画布数 = 1 + 1 = 2，乙为 1。
///   名称上限沿用 80 个 Unicode 字符；超长名 = 80 + 1 = 81 个字符。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 甲重复添加同目录，乙添加一次 → 创建三张独立画布。
///   2. 首张改为“  发布验收  ” → 去除首尾空白后保存。
///   3. 空白名、81 字符名和乙空间下的首张 ID → 全部拒绝，不覆盖有效名称。
///   4. 重开存储 → 读取首张、第二张和乙的画布。
///
/// 预期结果：
///   - 正断言：首张保存“发布验收”；第二张和乙仍为“project”。
///   - 负断言：首张旧名称不残留；改名不改变目录、项目 key 或乙的身份。
#[test]
fn project_canvas_rename_is_scoped_validated_and_persistent() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let alpha = store.create("Alpha").expect("应创建甲");
    let beta = store.create("Beta").expect("应创建乙");
    let first = store.add_project(&alpha.id, &project).expect("应创建首张");
    let second = store
        .add_project(&alpha.id, &project)
        .expect("应创建第二张");
    let other = store
        .add_project(&beta.id, &project)
        .expect("应创建乙的画布");
    store
        .rename_project(&alpha.id, &first.id, "  发布验收  ")
        .expect("应保存画布名称");
    assert!(
        store.rename_project(&alpha.id, &first.id, " \n ").is_err(),
        "空白名必须拒绝"
    );
    assert!(
        store
            .rename_project(&alpha.id, &first.id, &"名".repeat(81))
            .is_err(),
        "超过 80 个字符不能覆盖有效名称"
    );
    assert!(
        store
            .rename_project(&beta.id, &first.id, "错误覆盖")
            .is_err(),
        "不能通过另一空间改名"
    );
    let reopened = DesktopWorkspaceStore::new(path);
    let areas = reopened.list_projects(&alpha.id).expect("应重开甲");
    let renamed = areas
        .iter()
        .find(|area| area.id == first.id)
        .expect("首张应存在");
    assert_eq!(
        renamed.display_name, "发布验收",
        "自定义名称应去除首尾空白并持久化"
    );
    assert_ne!(renamed.display_name, first.display_name, "首张不得残留旧名");
    assert_eq!(renamed.root, first.root, "改名不能修改项目目录");
    assert_eq!(
        renamed.project_key, first.project_key,
        "改名不能改变项目身份"
    );
    assert_eq!(
        areas
            .iter()
            .find(|area| area.id == second.id)
            .expect("第二张应存在")
            .display_name,
        "project",
        "同项目第二张不应被改名"
    );
    assert_eq!(
        reopened.list_projects(&beta.id).expect("应重开乙")[0].id,
        other.id,
        "乙的画布身份不应改变"
    );
    assert_eq!(
        reopened.list_projects(&beta.id).expect("应重开乙")[0].display_name,
        "project",
        "乙不应被改名"
    );
}

/// 旧版项目去重数据库升级后支持重复添加，并保留已有画布、壁纸、派发特效和配置修订触发器。
///
/// 数据构造（含关键数值的推导过程）：
///   旧空间“legacy”、旧画布“original”、added_at=2（固定的历史时间）；旧配置 revision=7。
///   项目派发特效为 ribbon，语音来源覆盖为 fireworks。
///   升级不应计为编辑；新增画布后 revision = 7 + 1 = 8，改名后 = 8 + 1 = 9。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建带 UNIQUE 的旧表、配置修订和插入触发器 → 写入旧画布后将修订还原为 7。
///   2. 新 Store 读取 → 移除项目去重约束，保留特效列并重建原触发器，再迁移全局偏好。
///   3. 重复添加同项目 → 新 ID，修订变为 8；重命名新画布 → 修订变为 9。
///   4. 重开目录 → 保留两张画布，修订仍为 9。
///
/// 预期结果：
///   - 正断言：旧 ID、名称、时间、壁纸、派发特效及来源覆盖和展开视角保留，特效迁为全局，原触发器继续生效。
///   - 负断言：新画布不复用旧 ID；迁移不递增修订，重开不合并或删除画布。
#[test]
fn legacy_project_catalog_migration_preserves_data_and_revision_triggers() {
    use rusqlite::{params, Connection};
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let root = project.canonicalize().expect("应规范化路径");
    let path = temp.path().join("app.sqlite");
    let connection = Connection::open(&path).expect("应创建旧数据库");
    connection
        .execute_batch(
            "CREATE TABLE desktop_workspaces (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL,
            last_opened_at_unix_ms INTEGER NOT NULL, expanded_area_id TEXT,
            overview_scroll_y INTEGER NOT NULL DEFAULT 0);
         INSERT INTO desktop_workspaces VALUES('legacy','Workspace',1,1,'original',0);
         CREATE TABLE desktop_project_areas (
            id TEXT PRIMARY KEY NOT NULL,
            desktop_workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            project_key TEXT NOT NULL, root_path TEXT NOT NULL, display_name TEXT NOT NULL,
            added_at_unix_ms INTEGER NOT NULL, wallpaper_json TEXT, dispatch_effect_json TEXT,
            UNIQUE(desktop_workspace_id, project_key));
         CREATE TABLE desktop_configuration_revisions (
            workspace_id TEXT PRIMARY KEY REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL);
         INSERT INTO desktop_configuration_revisions VALUES('legacy',7);
         CREATE TRIGGER desktop_chat_project_added AFTER INSERT ON desktop_project_areas
            BEGIN UPDATE desktop_configuration_revisions SET revision=revision+1
            WHERE workspace_id=NEW.desktop_workspace_id; END;",
        )
        .expect("应构造旧版表和修订记录");
    connection
        .execute(
            "INSERT INTO desktop_project_areas VALUES('original','legacy',?1,?1,'原画布',2,?2,?3)",
            params![
                root.to_string_lossy(),
                r#"{"kind":"builtin","rel":"wallpapers/legacy.png"}"#,
                r#"{"style":"ribbon","voice":"fireworks"}"#
            ],
        )
        .expect("应写入旧画布");
    // Fixture 插入使 revision = 7 + 1；还原为验收起点 7。
    connection
        .execute("UPDATE desktop_configuration_revisions SET revision=7", [])
        .expect("应固定验收起点");
    drop(connection);
    let store = DesktopWorkspaceStore::new(path.clone());
    let before = store.list_projects("legacy").expect("应升级并读取旧画布");
    let effect = store
        .project_dispatch_effect("legacy", "original")
        .expect("旧特效仍可读取")
        .expect("旧特效应保留");
    assert_eq!(effect.style, "ribbon", "项目表重建不得丢掉派发特效列");
    assert_eq!(
        effect.voice.as_deref(),
        Some("fireworks"),
        "来源覆盖必须保留"
    );
    assert_eq!(
        storage::SettingsStore::new(path.clone())
            .get_string("effects.dispatch.v1")
            .expect("全局特效"),
        Some(serde_json::to_string(&effect).expect("JSON")),
        "老项目重建后仍应迁移成全局特效"
    );
    assert_eq!(before[0].id, "original", "升级不能换掉旧画布 ID");
    assert_eq!(before[0].display_name, "原画布", "升级不能重置旧名");
    assert_eq!(before[0].added_at_unix_ms, 2, "升级不能重置添加时间");
    assert_eq!(
        before[0].wallpaper,
        Some(storage::WallpaperReference::Builtin {
            rel: "wallpapers/legacy.png".into()
        }),
        "升级不能丢失壁纸"
    );
    assert_eq!(
        store.configuration_revision("legacy").expect("应读取修订"),
        7,
        "迁移复制不应触发用户配置编辑"
    );
    assert_eq!(
        store
            .get("legacy")
            .expect("应读取空间")
            .expect("空间应存在")
            .expanded_area_id,
        Some("original".into()),
        "升级应保留展开视角"
    );
    let new = store
        .add_project("legacy", &project)
        .expect("升级后应允许重复添加");
    assert_ne!(new.id, "original", "新画布不得复用旧 ID");
    assert_eq!(
        store.configuration_revision("legacy").expect("应读取修订"),
        8,
        "旧触发器升级后仍应递增修订"
    );
    store
        .rename_project("legacy", &new.id, "新画布")
        .expect("应改名新画布");
    let reopened = DesktopWorkspaceStore::new(path);
    assert_eq!(
        reopened
            .configuration_revision("legacy")
            .expect("应重开修订"),
        9,
        "改名应递增修订且重开不得重复迁移"
    );
    assert_eq!(
        reopened.list_projects("legacy").expect("应重开目录").len(),
        2,
        "重开不得合并或删除画布"
    );
}

/// 手机配置入口也允许重复添加同项目，修订锁仍拒绝过期请求。
///
/// 数据构造（含关键数值的推导过程）：
///   新空间初始 revision=1；两次添加后画布数=2，revision=1+1+1=3。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用当前 revision=1 添加 → 一张画布，修订变为 2。
///   2. 使用当前 revision=2 重复添加 → 两张独立画布，修订变为 3。
///   3. 再使用过期 revision=2 添加 → 拒绝请求，读取剩余画布数量。
///
/// 预期结果：
///   - 正断言：两次成功添加各递增一次修订，保留两张独立画布。
///   - 负断言：过期请求失败，不能生成第三张画布。
#[test]
fn revision_guarded_additions_allow_independent_instances() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let workspace = store.create("Workspace").expect("应创建空间");
    let first_revision = store
        .configuration_revision(&workspace.id)
        .expect("应读取初始修订");
    store
        .add_project_if_revision(&workspace.id, first_revision, &project)
        .expect("首次添加应成功");
    let second_revision = first_revision + 1;
    store
        .add_project_if_revision(&workspace.id, second_revision, &project)
        .expect("重复添加应成功");
    let areas = store.list_projects(&workspace.id).expect("应读取画布");
    assert_eq!(areas.len(), 2, "手机配置入口不应仍按项目去重");
    assert_ne!(areas[0].id, areas[1].id, "两张必须有独立身份");
    assert_eq!(
        store
            .configuration_revision(&workspace.id)
            .expect("应读取修订"),
        first_revision + 2,
        "两次添加必须各递增一次修订"
    );
    assert!(
        store
            .add_project_if_revision(&workspace.id, second_revision, &project)
            .is_err(),
        "旧修订请求不得继续添加"
    );
    assert_eq!(
        store
            .list_projects(&workspace.id)
            .expect("应读取剩余画布")
            .len(),
        2,
        "旧修订请求不得生成第三张画布"
    );
}

/// DWS-02/DWS-05：同一仓库的两个 Git worktree 保留各自独立画布，另一工作空间也有独立区域。
///
/// 数据构造（含关键数值的推导过程）：
///   Git 仓库数 = 1，工作树目录数 = 2，桌面工作空间数 = 2。
///   甲中两次添加同仓库 → 甲区域数 = 2；乙中添加同仓库 → 乙区域数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建带一次空提交的 Git 仓库，并添加链接工作树。
///   2. 甲先添加主工作树再添加链接工作树 → 创建两张独立画布并保留各自目录。
///   3. 乙添加链接工作树 → 创建属于乙的新区域；重开数据库核对数量。
///
/// 预期结果：
///   - 正断言：甲有两处区域、乙有一处，共用仓库 key，各自保留工作树根目录。
///   - 负断言：甲的两处区域不共用身份，乙的区域不混入甲。
#[test]
fn desktop_project_area_preserves_independent_git_worktree_canvases() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let main = temp.path().join("main");
    let linked = temp.path().join("linked");
    std::fs::create_dir(&main).expect("测试应创建仓库目录");
    for args in [
        vec!["init", "--initial-branch=main"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ],
    ] {
        let output = Command::new("git")
            .current_dir(&main)
            .args(&args)
            .output()
            .expect("测试应可调用 git");
        assert!(
            output.status.success(),
            "git {args:?} 应成功：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new("git")
        .current_dir(&main)
        .args([
            "worktree",
            "add",
            "-b",
            "feature",
            linked.to_str().expect("临时目录应为 UTF-8"),
        ])
        .output()
        .expect("测试应可调用 git worktree");
    assert!(
        output.status.success(),
        "测试应建立第二工作树：{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let alpha = store.create("Alpha").expect("应创建 Alpha");
    let beta = store.create("Beta").expect("应创建 Beta");
    let first = store
        .add_project(&alpha.id, &main)
        .expect("甲应添加主工作树");
    let duplicate = store
        .add_project(&alpha.id, &linked)
        .expect("甲应创建独立工作树画布");
    let other = store
        .add_project(&beta.id, &linked)
        .expect("乙应创建独立区域");

    let alpha_projects = store.list_projects(&alpha.id).expect("应读取甲的区域");
    let beta_projects = store.list_projects(&beta.id).expect("应读取乙的区域");
    assert_eq!(alpha_projects.len(), 2, "甲中同 Git 仓库应有两张独立画布");
    assert_eq!(beta_projects.len(), 1, "乙中应有独立的一个区域");
    assert_ne!(first.id, duplicate.id, "甲中重复添加不得复用区域身份");
    assert_eq!(
        first.root,
        main.canonicalize().unwrap(),
        "首张应保留主工作树根目录"
    );
    assert_eq!(
        duplicate.root,
        linked.canonicalize().unwrap(),
        "第二张应使用链接工作树目录"
    );
    assert_ne!(first.id, other.id, "另一工作空间应创建新的区域身份");
    assert_eq!(
        first.project_key, other.project_key,
        "两空间引用相同项目身份"
    );
    assert_eq!(
        store
            .get(&alpha.id)
            .expect("应读取甲")
            .expect("甲应存在")
            .project_count,
        2,
        "甲的目录卡片应显示两张项目画布"
    );
    assert!(
        !alpha_projects.iter().any(|area| area.id == other.id),
        "乙的区域不得出现在甲中"
    );
}
