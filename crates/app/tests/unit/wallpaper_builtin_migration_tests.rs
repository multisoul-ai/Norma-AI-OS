use super::state::{WallpaperSelection, WallpaperSettingsState};
use super::tests::TestRootGuard;

/// T-WM1: 已持久化的旧版 PNG 内置壁纸在加载时迁移到 WebP，并回写 canonical 路径。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy kind  = "builtin"                       （1 条类型记录）
///   legacy value = "wallpapers/1350954.png"        （1 条旧路径记录）
///   canonical    = "wallpapers/1350954.webp"       （同 basename，仅格式变化）
///   settings rows = kind(1) + value(1) = 2 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离 SQLite 中预写旧版 kind/value 两条记录。
///   2. WallpaperSettingsState::load_from_store 读取旧 PNG 路径。
///   3. 内置路径迁移表把该选择映射为同名 WebP，并验证新资源可解析。
///   4. state 将 canonical WebP 路径回写到同一 SQLite。
///
/// 预期结果：
///   - 正断言：内存选择精确为 wallpapers/1350954.webp，用户仍看到 Aurora 壁纸。
///   - 正断言：数据库 value 精确回写为 wallpapers/1350954.webp。
///   - 负断言：数据库不得继续保留已从安装包移除的 wallpapers/1350954.png。
#[test]
fn test_legacy_builtin_png_selection_migrates_and_persists_webp_path() {
    let root = TestRootGuard::new();
    let seed_store = root.settings_store();
    seed_store
        .set_string("wallpaper.kind", "builtin")
        .expect("test setup must persist the legacy builtin kind");
    seed_store
        .set_string("wallpaper.value", "wallpapers/1350954.png")
        .expect("test setup must persist the legacy PNG wallpaper path");

    let state = WallpaperSettingsState::load_from_store(root.settings_store());
    assert_eq!(
        state.selection,
        WallpaperSelection::Builtin {
            rel: "wallpapers/1350954.webp"
        },
        "loading a legacy Aurora PNG selection must preserve it as the canonical WebP"
    );

    let persisted = root
        .settings_store()
        .get_string("wallpaper.value")
        .expect("migrated wallpaper value must remain readable")
        .expect("migrated wallpaper value must exist");
    assert_eq!(
        persisted, "wallpapers/1350954.webp",
        "migration must write the canonical WebP path back to SQLite"
    );
    assert_ne!(
        persisted, "wallpapers/1350954.png",
        "migration must not leave the removed legacy PNG path in SQLite"
    );
}

/// 场景：旧项目素材失效，升级后继续使用原先回退到的工作空间壁纸。
/// 数据构造：空间使用 Aurora，展开项目引用不存在的内置资源；模拟未迁移数据库。
/// 执行过程：加载全局壁纸状态，触发存储迁移和资源验证，再重新加载。
/// 预期结果：两次均显示 Aurora，全局存档不保留失效资源。
#[test]
fn global_migration_preserves_workspace_fallback_for_invalid_project_wallpaper() {
    let root = TestRootGuard::new();
    let settings = root.settings_store();
    let catalog = storage::DesktopWorkspaceStore::new(settings.path().to_owned());
    let space = catalog
        .create_with_wallpaper(
            "Recent",
            &storage::WallpaperReference::Builtin {
                rel: "wallpapers/1350954.webp".into(),
            },
        )
        .expect("空间");
    let project = tempfile::tempdir().expect("项目目录");
    let area = catalog
        .add_project(&space.id, project.path())
        .expect("项目");
    let connection = rusqlite::Connection::open(settings.path()).expect("旧库");
    connection
        .execute(
            "UPDATE desktop_project_areas SET wallpaper_json=?1 WHERE id=?2",
            [
                r#"{"kind":"builtin","rel":"wallpapers/obsolete.png"}"#,
                &area.id,
            ],
        )
        .expect("旧失效素材");
    connection
        .execute(
            "UPDATE desktop_workspaces SET expanded_area_id=?1 WHERE id=?2",
            [&area.id, &space.id],
        )
        .expect("展开项目");
    connection
        .execute(
            "DELETE FROM settings WHERE key='appearance.global_migrated.v1'",
            [],
        )
        .expect("模拟升级前");
    drop(connection);
    for _ in 0..2 {
        let state = WallpaperSettingsState::load_from_store(settings.clone());
        assert_eq!(
            state.selection,
            WallpaperSelection::Builtin {
                rel: "wallpapers/1350954.webp"
            },
            "升级和重开均应保留旧界面实际使用的空间壁纸"
        );
    }
    assert_eq!(
        settings.get_string("wallpaper.value").expect("全局值"),
        Some("wallpapers/1350954.webp".into()),
        "失效项目素材不得保留为全局选择"
    );
}
