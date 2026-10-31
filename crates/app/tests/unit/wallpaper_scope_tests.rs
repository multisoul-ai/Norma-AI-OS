use super::preferences::WallpaperPreferences;
use super::state::WallpaperSelection;

/// 场景：自定义全局壁纸丢失时安全回退，文件再次可用时恢复原选择。
/// 数据构造：选择 mine.png，缓存中起初没有文件，之后加入一个同名文件。
/// 执行过程：读取有效壁纸 → 更新缓存 → 再读取。
/// 预期结果：先默认后自定义；回退不抹掉用户的全局选择。
#[test]
fn global_wallpaper_missing_file_falls_back_without_erasing_preference() {
    let selected = WallpaperSelection::Custom {
        filename: "mine.png".into(),
    };
    let mut state = WallpaperPreferences::new(selected.clone(), Vec::new());
    assert_eq!(
        state.effective_selection(),
        WallpaperSelection::default(),
        "缺失时应回退默认"
    );
    assert_eq!(state.global, selected, "缺失不抹掉全局选择");
    state.customs.push("mine.png".into());
    assert_eq!(state.effective_selection(), selected, "可用后恢复全局壁纸");
    assert_ne!(
        state.effective_selection(),
        WallpaperSelection::default(),
        "不得仍停留在默认壁纸"
    );
}
