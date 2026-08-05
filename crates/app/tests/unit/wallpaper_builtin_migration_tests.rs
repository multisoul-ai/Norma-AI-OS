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
