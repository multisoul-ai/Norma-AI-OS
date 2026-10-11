use super::state::{WallpaperSelection, WallpaperSettingsState};
use super::tests::TestRootGuard;

// 场景：升级轻量包后，已保存的动态壁纸尚未下载。
// 在隔离的空 .app bundle 中运行同一测试二进制，预写 Tide 选择到临时 SQLite。
// 预期仍保存 Tide，解析路径为 None 供默认图回退，不把用户选择改写成默认壁纸。
#[test]
fn saved_optional_wallpaper_survives_missing_packaged_media() {
    const CHILD: &str = "NORMA_OPTIONAL_MEDIA_UNIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let fixture = tempfile::tempdir().expect("isolated bundle fixture must be created");
        let contents = fixture.path().join("Fixture.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS"))
            .expect("bundle executable folder must be created");
        std::fs::create_dir(contents.join("Resources"))
            .expect("bundle must have an empty resource folder");
        let executable = contents.join("MacOS/media-choice-test");
        std::fs::copy(
            std::env::current_exe().expect("current test binary must be known"),
            &executable,
        )
        .expect("test binary must be copied into isolated bundle");
        let result = std::process::Command::new(executable)
            .args([
                "saved_optional_wallpaper_survives_missing_packaged_media",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .expect("isolated bundle test must execute");
        assert!(
            result.status.success(),
            "missing-media persistence test must pass inside bundle: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let fixture = TestRootGuard::new();
    let store = fixture.settings_store();
    store
        .set_string("wallpaper.kind", "builtin")
        .expect("selection kind must be seeded");
    store
        .set_string("wallpaper.value", "wallpapers/1403512.mp4")
        .expect("Tide selection must be seeded");
    let state = WallpaperSettingsState::load_from_store(store.clone());
    assert_eq!(
        state.selection,
        WallpaperSelection::Builtin {
            rel: "wallpapers/1403512.mp4"
        },
        "missing optional file cannot erase the persisted user selection"
    );
    assert_eq!(
        state.resolve_path(),
        None,
        "undownloaded original media must remain unavailable"
    );
    assert_eq!(
        store
            .get_string("wallpaper.value")
            .expect("saved wallpaper must remain readable"),
        Some("wallpapers/1403512.mp4".into()),
        "offline fallback cannot overwrite SQLite selection"
    );
}
