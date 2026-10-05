//! Task 4: WallpaperSettingsState persistence tests.
//!
//! These tests exercise the persistent wallpaper selection state that mirrors the
//! voice settings pattern. Each test passes an explicit tempdir-backed
//! `SettingsStore`, so the production app database is never touched.
//!
//! Concurrency note: `MULTISOUL_TEST_DATA_ROOT` is a process-global env var and
//! cargo executes tests in parallel threads within the same process. To avoid
//! races between tests that mutate this var, every test acquires a process-wide
//! [`TEST_ROOT_LOCK`] Mutex guard before setting the env var, and the guard
//! clears the var on drop. This serializes the (small number of) wallpaper
//! persistence tests while leaving all other crate tests fully parallel.

use super::dirs::custom_wallpapers_dir;
use super::import::ImportResult;
use super::state::{WallpaperSelection, WallpaperSettingsState};
use std::fs;
use std::sync::Mutex;

/// Process-wide lock acquired by every wallpaper persistence test before
/// touching `MULTISOUL_TEST_DATA_ROOT`. Poisoning is treated as a hard failure.
///
/// `pub(super)` so the sibling `render_id_tests` module (also under
/// `settings::wallpaper`) can share the same serialization guard.
pub(super) static TEST_ROOT_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard. On construction it acquires [`TEST_ROOT_LOCK`], allocates a fresh
/// tempdir, and points `MULTISOUL_TEST_DATA_ROOT` at it. On drop it removes the
/// env var and releases the lock, so the next test starts from a clean slate.
pub(super) struct TestRootGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    tmp: tempfile::TempDir,
}

impl TestRootGuard {
    pub(super) fn new() -> Self {
        let guard = TEST_ROOT_LOCK
            .lock()
            .expect("wallpaper test root mutex poisoned");
        let tmp = tempfile::tempdir().expect("tempdir for wallpaper test root");
        std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
        Self { _guard: guard, tmp }
    }

    pub(super) fn settings_store(&self) -> storage::SettingsStore {
        storage::SettingsStore::new(self.tmp.path().join("app.sqlite"))
    }
}

impl Drop for TestRootGuard {
    fn drop(&mut self) {
        std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
    }
}

/// T-W1: load_default 在无偏好时返回默认内置壁纸 1331865.webp。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT → app.sqlite 不存在
///   custom wallpapers dir = tempdir/wallpapers（不存在）
///   settings_store 表无 wallpaper.kind / wallpaper.value 记录
///
/// 执行过程：
///   1. WallpaperSettingsState::load_from_store(tempdir/app.sqlite)
///   2. SavedWallpaper::load 读取两条 KEY → 均 None → 回退默认 selection
///
/// 预期结果：
///   - 正断言：默认 selection 是 Builtin{rel ends_with("1331865.webp")}
///   - 正断言：current_kind() == Static（默认壁纸是 WebP）
#[test]
fn test_load_default_falls_back_to_default_wallpaper() {
    let _g = TestRootGuard::new();
    let s = WallpaperSettingsState::load_from_store(_g.settings_store());
    assert!(
        matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.webp")),
        "default selection must be the embedded default wallpaper"
    );
    assert_eq!(
        s.current_kind(),
        crate::app::wallpaper_resolvers::WallpaperKind::Static,
        "default wallpaper kind must be Static"
    );
}

/// T-W2: select_builtin 持久化后，新建 state 读取应还原同一选择。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT → 两个独立 WallpaperSettingsState 实例
///     共享同一个 app.sqlite（同一路径，settings_store 每次重新打开连接）
///   select_builtin("wallpapers/220240.gif") → 写入两条 KEY
///
/// 执行过程：
///   1. s = load_from_store(tempdir/app.sqlite) → 默认壁纸
///   2. s.select_builtin("wallpapers/220240.gif") → 持久化 kind=builtin,value=wallpapers/220240.gif
///   3. s2 = load_from_store(同一路径) → 重新打开 store 读取 → 还原 Builtin{220240.gif}
///
/// 预期结果：
///   - 正断言：s2.selection == Builtin{rel == "wallpapers/220240.gif"}
///   - 负断言：s2.selection 不应仍是默认 1331865.webp
#[test]
fn test_builtin_selection_round_trips_through_persistence() {
    let _g = TestRootGuard::new();

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    s.select_builtin("wallpapers/220240.gif");

    let s2 = WallpaperSettingsState::load_from_store(_g.settings_store());
    assert!(
        matches!(&s2.selection, WallpaperSelection::Builtin { rel } if rel == &"wallpapers/220240.gif"),
        "reloaded selection must be the gif builtin we just saved"
    );
    assert!(
        !matches!(&s2.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.webp")),
        "reloaded selection must NOT be the default after explicit select_builtin"
    );
}

/// T-W3: delete_custom 删除当前选中的自定义项后，selection 回退默认；文件确实被删。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   custom dir = tempdir/wallpapers/，预写入 mine.png（内容 b"fake"）
///   select_custom("mine.png") → selection=Custom{mine.png}，持久化两条 KEY
///
/// 执行过程：
///   1. s.select_custom("mine.png") → selection=Custom 且两条 KEY 写盘
///   2. s.delete_custom("mine.png") → 删 tempdir/wallpapers/mine.png；因 custom 是当前选中
///      → 回退默认 Builtin{1331865.webp} 并持久化
///
/// 预期结果：
///   - 正断言：删后 selection 是 Builtin{ends_with("1331865.webp")}
///   - 正断言：mine.png 文件已从磁盘移除
///   - 负断言：删后 selection 不应仍为 Custom
#[test]
fn test_delete_custom_falls_back_to_default_and_removes_file() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("mine.png"), b"fake").expect("write fake custom wallpaper");

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    s.select_custom("mine.png".to_string());
    s.delete_custom("mine.png");

    assert!(
        matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.webp")),
        "deleting the in-use custom must fall back to default"
    );
    assert!(
        !dir.join("mine.png").exists(),
        "custom file must be removed from disk"
    );
}

/// T-W4: select_custom 持久化后新 state 也应还原 Custom 选择。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   custom dir = tempdir/wallpapers/，预写入 night.jpg
///   select_custom("night.jpg") 持久化两条 KEY
///
/// 执行过程：
///   1. s.select_custom("night.jpg") → 持久化 kind=custom,value=night.jpg
///   2. s2 = load_from_store(同一路径) → 重读 → 还原 Custom{night.jpg}
///
/// 预期结果：
///   - 正断言：s2.selection == Custom{filename == "night.jpg"}
///   - 正断言：s2.current_kind() == Static（.jpg 扩展名归类为 Static）
#[test]
fn test_custom_selection_round_trips_through_persistence() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("night.jpg"), b"fake-jpg").expect("write fake custom jpg");

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    s.select_custom("night.jpg".to_string());

    let s2 = WallpaperSettingsState::load_from_store(_g.settings_store());
    assert!(
        matches!(&s2.selection, WallpaperSelection::Custom { filename } if filename == "night.jpg"),
        "reloaded selection must restore the custom filename we just saved"
    );
    assert_eq!(
        s2.current_kind(),
        crate::app::wallpaper_resolvers::WallpaperKind::Static,
        ".jpg custom wallpaper should classify as Static"
    );
}

/// T-W5: 内置与自定义 selection 都通过磁盘路径解析，缺失文件返回 None。
///
/// 数据构造（含关键数值的推导过程）：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   builtin files = default WebP + GIF + MP4 = 3 个真实资源路径
///   custom files  = mine.png(存在) + ghost.png(缺失) = 2 个候选路径
///
/// 执行过程：
///   1. 分别选择默认 WebP、内置 GIF 与内置 MP4，调用 resolve_path
///   2. 选择存在的 mine.png 与缺失的 ghost.png，调用 resolve_path
///
/// 预期结果：
///   - 正断言：3 个内置资源分别解析到对应磁盘文件
///   - 存在的 custom：resolve_path 返回 Some 且 ends_with("mine.png")
///   - 负断言：缺失的 custom 不得伪造可读取路径
#[test]
fn test_resolve_path_uses_disk_resources_for_builtin_and_custom_wallpapers() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("mine.png"), b"fake").expect("write fake custom wallpaper");

    // Default Static builtin.
    let s_default = WallpaperSettingsState::load_from_store(_g.settings_store());
    let default_path = s_default
        .resolve_path()
        .expect("default builtin must resolve to a real on-disk path in dev");
    assert!(
        default_path.ends_with("wallpapers/1331865.webp"),
        "default Static builtin must resolve to wallpapers/1331865.webp; got {default_path:?}"
    );

    // Gif builtin.
    let mut s_gif = WallpaperSettingsState::load_from_store(_g.settings_store());
    s_gif.select_builtin("wallpapers/220240.gif");
    let gif_path = s_gif
        .resolve_path()
        .expect("Gif builtin must resolve to a real on-disk resource");
    assert!(
        gif_path.ends_with("wallpapers/220240.gif"),
        "Gif builtin must resolve to wallpapers/220240.gif; got {gif_path:?}"
    );

    // Video builtin uses the same resource resolver even though its live frame
    // is rendered through AVFoundation instead of GPUI's image loader.
    let mut s_video = WallpaperSettingsState::load_from_store(_g.settings_store());
    s_video.select_builtin("wallpapers/1403512.mp4");
    assert_eq!(
        s_video.current_kind(),
        crate::app::wallpaper_resolvers::WallpaperKind::Video,
        "the mp4 builtin must classify as Video"
    );
    let video_path = s_video
        .resolve_path()
        .expect("video builtin must resolve to a real on-disk resource");
    assert!(
        video_path.ends_with("wallpapers/1403512.mp4"),
        "video builtin must resolve to wallpapers/1403512.mp4; got {video_path:?}"
    );

    // Existing custom: resolve_path Some.
    let mut s_custom = WallpaperSettingsState::load_from_store(_g.settings_store());
    s_custom.select_custom("mine.png".to_string());
    let resolved = s_custom
        .resolve_path()
        .expect("existing custom file must resolve");
    assert!(
        resolved.to_string_lossy().ends_with("mine.png"),
        "custom resolve_path must end with the custom filename; got {resolved:?}"
    );
    // Missing custom: resolve_path None.
    let mut s_missing = WallpaperSettingsState::load_from_store(_g.settings_store());
    s_missing.select_custom("ghost.png".to_string());
    assert_eq!(
        s_missing.resolve_path(),
        None,
        "missing custom file must not resolve"
    );
}

/// T-W6: list_custom 扫描数据目录并按扩展名过滤；delete 非选中的 custom 不影响当前 selection。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   custom dir 写入：valid.png, anim.gif, clip.mp4, ignore.txt
///
/// 执行过程：
///   1. list_custom() → 返回 valid.png / anim.gif / clip.mp4，不含 ignore.txt
///   2. selection 选 valid.png（custom）
///   3. delete_custom("anim.gif")（非当前选中文件）→ 删 anim.gif，selection 仍是 valid.png
///
/// 预期结果：
///   - 正断言：list_custom 包含 valid.png / anim.gif / clip.mp4
///   - 负断言：list_custom 不包含 ignore.txt
///   - 正断言：删除非当前 custom 后 selection 不变
///   - 正断言：anim.gif 已从磁盘移除
#[test]
fn test_list_custom_filters_and_delete_non_current_keeps_selection() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("valid.png"), b"v").expect("write valid.png");
    fs::write(dir.join("anim.gif"), b"a").expect("write anim.gif");
    fs::write(dir.join("clip.mp4"), b"c").expect("write clip.mp4");
    fs::write(dir.join("ignore.txt"), b"x").expect("write ignore.txt");

    let s = WallpaperSettingsState::load_from_store(_g.settings_store());
    let listed = s.list_custom();
    assert!(
        listed.contains(&"valid.png".to_string()),
        "list_custom must include valid.png; got {listed:?}"
    );
    assert!(
        listed.contains(&"anim.gif".to_string()),
        "list_custom must include anim.gif; got {listed:?}"
    );
    assert!(
        listed.contains(&"clip.mp4".to_string()),
        "list_custom must include clip.mp4; got {listed:?}"
    );
    assert!(
        !listed.contains(&"ignore.txt".to_string()),
        "list_custom must NOT include non-wallpaper extensions like .txt; got {listed:?}"
    );

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    s.select_custom("valid.png".to_string());
    s.delete_custom("anim.gif");

    assert!(
        matches!(s.selection, WallpaperSelection::Custom { filename } if filename == "valid.png"),
        "deleting a non-current custom must NOT change the current selection"
    );
    assert!(
        !dir.join("anim.gif").exists(),
        "anim.gif must have been removed from disk"
    );
    assert!(
        dir.join("valid.png").exists(),
        "valid.png must still exist after deleting a different file"
    );
}

/// T-W7: import_custom 把一个 png 拷到数据目录，列表出现，selection 切到它。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT → custom dir = tempdir/wallpapers（不存在）
///   额外独立 tempdir 提供 src.png（内容 b"fakepng"），与壁纸目录隔离避免污染
///   WallpaperSettingsState::load_from_store(tempdir/app.sqlite) → 默认 builtin 选择
///
/// 执行过程：
///   1. s.import_custom(src.png)
///      → 扩展 png → Static ✓
///      → create_dir_all(tempdir/wallpapers)
///      → 非 colliding dest = tempdir/wallpapers/src.png
///      → fs::copy(src → dest) 成功
///      → select_custom("src.png") 持久化
///   2. list_custom() 扫描 tempdir/wallpapers → [src.png]
///
/// 预期结果：
///   - 正断言：返回 ImportResult::Imported(_)
///   - 正断言：list_custom 长度 1
///   - 正断言：selection == Custom { .. }
///   - 正断言：tempdir/wallpapers/src.png 物理存在
#[test]
fn test_import_custom_copies_and_selects() {
    let _g = TestRootGuard::new();
    // Source lives in a separate tempdir so it never appears as a wallpaper.
    let src_tmp = tempfile::tempdir().expect("tempdir for source file");
    let src = src_tmp.path().join("src.png");
    fs::write(&src, b"fakepng").expect("write source png");

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    let res = s.import_custom(&src);
    assert!(
        matches!(res, ImportResult::Imported(_)),
        "png import must succeed; got {res:?}"
    );

    let custom = s.list_custom();
    assert_eq!(
        custom.len(),
        1,
        "exactly one custom wallpaper after import; got {custom:?}"
    );
    assert!(
        matches!(s.selection, WallpaperSelection::Custom { .. }),
        "selection must switch to the imported custom wallpaper"
    );

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    assert!(
        dir.join("src.png").is_file(),
        "imported file must physically exist in the custom wallpapers dir"
    );
}

/// T-W8: import_custom 拒绝非图片/非视频扩展（.txt），返回 Rejected 且不污染目录。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   独立 src.tmp/notes.txt（内容 b"nope"）
///   load_from_store(tempdir/app.sqlite) → 默认 builtin
///
/// 执行过程：
///   s.import_custom(notes.txt)
///     → 扩展 txt → WallpaperKind::from_extension 返回 None
///     → 立即返回 Rejected，不触 create_dir_all/copy
///
/// 预期结果：
///   - 正断言：返回 ImportResult::Rejected(_)
///   - 正断言：list_custom 为空（无文件残留）
///   - 负断言：selection 不被切到 Custom
#[test]
fn test_import_custom_rejects_unknown_extension() {
    let _g = TestRootGuard::new();
    let src_tmp = tempfile::tempdir().expect("tempdir for source file");
    let src = src_tmp.path().join("notes.txt");
    fs::write(&src, b"nope").expect("write source txt");

    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());
    let res = s.import_custom(&src);
    assert!(
        matches!(res, ImportResult::Rejected(_)),
        "txt import must be rejected; got {res:?}"
    );
    assert!(
        s.list_custom().is_empty(),
        "rejected import must not leave any file in the custom dir"
    );
    assert!(
        !matches!(s.selection, WallpaperSelection::Custom { .. }),
        "rejected import must not switch selection to Custom"
    );
}

/// T-W9: 壁纸页保留素材网格、导入入口及自定义素材删除入口。
///
/// 数据构造：view.rs + cells.rs 两个源文件，构成完整页面与缩略图控件。
/// 执行过程：检查网格、导入、删除标识，以及单帧缓存渲染约定。
/// 预期结果：上述入口保持存在；缩略图不会改为逐帧动画渲染。
///
/// 这是结构性的源码锚点测试，避免 view 被误重构时丢失关键交互元素。
/// T-W10（render_id_key 契约）见 wallpaper_render_id_tests.rs，独立文件以守住 500 行上限。

#[test]
fn test_wallpaper_view_has_grid_import_and_builtin_nodelete() {
    let src = concat!(
        include_str!("../../src/settings/wallpaper/view.rs"),
        include_str!("../../src/settings/wallpaper/cells.rs")
    );
    assert!(
        src.contains("wallpaper-thumb"),
        "view must define builtin thumbnail cell anchors (id \"wallpaper-thumb\")"
    );
    assert!(
        src.contains("settings.import_formats"),
        "view must retain the localized import description"
    );
    assert!(
        src.contains("wallpaper-import"),
        "view must give the Import button the wallpaper-import id"
    );
    assert!(
        src.contains("wallpaper-delete"),
        "view must define a per-cell delete affordance id (wallpaper-delete)"
    );
    assert!(
        src.contains("single-frame cache"),
        "library previews must retain the bounded single-frame rendering contract"
    );
}
