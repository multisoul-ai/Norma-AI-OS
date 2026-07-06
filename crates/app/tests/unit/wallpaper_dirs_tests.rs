use super::dirs::custom_wallpapers_dir_from_roots;
use std::fs;

/// T-D1: 生产数据根迁移到 Norma OS 时，应复制旧 Multisoul wallpapers 目录。
///
/// 数据构造：
///   current_root = temp/Library/Application Support/Norma OS，初始不存在 wallpapers
///   legacy_root = temp/Library/Application Support/Multisoul，存在 wallpapers/old.png
///   old.png 内容 = b"legacy-wallpaper"，共 16 bytes
///
/// 执行过程：
///   1. 在 legacy_root/wallpapers/old.png 写入旧自定义壁纸
///   2. custom_wallpapers_dir_from_roots(current_root, Some(legacy_root))
///      → current wallpapers 不存在，legacy wallpapers 存在，复制目录
///   3. 读取 current_root/wallpapers/old.png 和 legacy_root/wallpapers/old.png
///
/// 预期结果：
///   - 正断言：返回目录为 current_root/wallpapers
///   - 正断言：current old.png 内容等于 b"legacy-wallpaper"
///   - 正断言：legacy old.png 仍保留
///   - 负断言：返回目录不应继续指向 legacy_root/wallpapers
#[test]
fn test_custom_wallpapers_dir_migrates_legacy_wallpapers_without_deleting_them() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let current_root = temp
        .path()
        .join("Library")
        .join("Application Support")
        .join("Norma OS");
    let legacy_root = temp
        .path()
        .join("Library")
        .join("Application Support")
        .join("Multisoul");
    let legacy_wallpapers = legacy_root.join("wallpapers");
    fs::create_dir_all(&legacy_wallpapers).expect("test should create legacy wallpapers dir");
    fs::write(legacy_wallpapers.join("old.png"), b"legacy-wallpaper")
        .expect("test should write legacy wallpaper");

    let migrated_dir = custom_wallpapers_dir_from_roots(&current_root, Some(&legacy_root))
        .expect("custom wallpapers dir 应能从 current/legacy roots 推导");
    let migrated_bytes =
        fs::read(current_root.join("wallpapers").join("old.png")).expect("old.png 应复制到新目录");
    let legacy_bytes =
        fs::read(legacy_root.join("wallpapers").join("old.png")).expect("old.png 应留在旧目录");

    assert_eq!(
        migrated_dir,
        current_root.join("wallpapers"),
        "迁移后应返回 Norma OS wallpapers 目录"
    );
    assert_eq!(
        migrated_bytes, b"legacy-wallpaper",
        "Norma OS wallpapers 目录应包含从旧目录复制的自定义壁纸"
    );
    assert_eq!(
        legacy_bytes, b"legacy-wallpaper",
        "迁移不应删除旧 Multisoul wallpapers/old.png"
    );
    assert_ne!(
        migrated_dir,
        legacy_root.join("wallpapers"),
        "迁移成功后不应继续把旧 Multisoul wallpapers 当作默认目录"
    );
}
