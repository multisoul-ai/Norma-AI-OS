use super::scoped::{ScopedWallpaperState, WallpaperContext};
use super::state::WallpaperSelection;
use storage::{DesktopWorkspaceStore, WallpaperReference};

/// WP-05：有效项目覆盖优先，缺失素材回退空间，清除覆盖恢复跟随。
///
/// 数据构造：工作空间 1、项目区域 1；空间使用内置默认，项目引用缺失 png。
///   可用自定义文件 = 0，加入 mine.png 后 = 0 + 1 = 1。
/// 执行过程：绑定缺失覆盖 → 加入可用文件 → 清除覆盖 → 再改变空间壁纸。
/// 预期结果：缺失时回退且原引用保留；有效时显示覆盖；清除后跟随空间，不再显示 png。
#[test]
fn effective_wallpaper_resolves_override_then_workspace_without_erasing_missing_choice() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let store = DesktopWorkspaceStore::new(temp.path().join("app.sqlite"));
    let default = WallpaperSelection::default();
    let workspace = store
        .create_with_wallpaper("Alpha", &default.reference())
        .expect("应创建空间");
    let mut area = store
        .add_project(&workspace.id, &root)
        .expect("应创建项目区域");
    area.wallpaper = Some(WallpaperReference::Custom {
        filename: "mine.png".into(),
    });
    let mut state = ScopedWallpaperState::new(default.clone(), Vec::new());
    state.bind(Some(WallpaperContext {
        workspace,
        area: Some(area),
    }));
    assert_eq!(
        state.effective_selection(),
        default,
        "缺失覆盖应回退空间壁纸"
    );
    assert!(
        state
            .context
            .as_ref()
            .expect("应有上下文")
            .area
            .as_ref()
            .expect("应有区域")
            .wallpaper
            .is_some(),
        "回退不得抹掉原选择"
    );
    state.customs.push("mine.png".into());
    assert_eq!(
        state.effective_selection(),
        WallpaperSelection::Custom {
            filename: "mine.png".into()
        },
        "可用覆盖应优先于空间壁纸"
    );
    state
        .context
        .as_mut()
        .expect("应有上下文")
        .area
        .as_mut()
        .expect("应有区域")
        .wallpaper = None;
    assert_eq!(state.effective_selection(), default, "清除覆盖应恢复跟随");
    assert_ne!(
        state.effective_selection(),
        WallpaperSelection::Custom {
            filename: "mine.png".into()
        },
        "跟随模式不得保留覆盖"
    );
}
