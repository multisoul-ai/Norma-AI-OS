//! Task 4: WallpaperSettingsState persistence tests.
//!
//! These tests exercise the persistent wallpaper selection state that mirrors the
//! voice settings pattern. They run against a per-test tempdir routed via the
//! `MULTISOUL_TEST_DATA_ROOT` env var so the production app data dir is never
//! touched.
//!
//! Concurrency note: `MULTISOUL_TEST_DATA_ROOT` is a process-global env var and
//! cargo executes tests in parallel threads within the same process. To avoid
//! races between tests that mutate this var, every test acquires a process-wide
//! [`TEST_ROOT_LOCK`] Mutex guard before setting the env var, and the guard
//! clears the var on drop. This serializes the (small number of) wallpaper
//! persistence tests while leaving all other crate tests fully parallel.

use super::import::ImportResult;
use super::state::{custom_wallpapers_dir, WallpaperSelection, WallpaperSettingsState};
use std::fs;
use std::sync::Mutex;

/// Process-wide lock acquired by every wallpaper persistence test before
/// touching `MULTISOUL_TEST_DATA_ROOT`. Poisoning is treated as a hard failure.
static TEST_ROOT_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard. On construction it acquires [`TEST_ROOT_LOCK`], allocates a fresh
/// tempdir, and points `MULTISOUL_TEST_DATA_ROOT` at it. On drop it removes the
/// env var and releases the lock, so the next test starts from a clean slate.
struct TestRootGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    _tmp: tempfile::TempDir,
}

impl TestRootGuard {
    fn new() -> Self {
        let guard = TEST_ROOT_LOCK
            .lock()
            .expect("wallpaper test root mutex poisoned");
        let tmp = tempfile::tempdir().expect("tempdir for wallpaper test root");
        std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
        Self {
            _guard: guard,
            _tmp: tmp,
        }
    }
}

impl Drop for TestRootGuard {
    fn drop(&mut self) {
        std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
    }
}

/// T-W1: load_default 在无偏好时返回默认内置壁纸 1331865.png。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT → app.sqlite 不存在
///   custom wallpapers dir = tempdir/wallpapers（不存在）
///   settings_store 表无 wallpaper.kind / wallpaper.value 记录
///
/// 执行过程：
///   1. WallpaperSettingsState::load_default_with_test_store() 打开 tempdir/app.sqlite
///   2. SavedWallpaper::load 读取两条 KEY → 均 None → 回退默认 selection
///
/// 预期结果：
///   - 正断言：默认 selection 是 Builtin{rel ends_with("1331865.png")}
///   - 正断言：current_kind() == Static（默认壁纸是 PNG）
#[test]
fn test_load_default_falls_back_to_default_wallpaper() {
    let _g = TestRootGuard::new();
    let s = WallpaperSettingsState::load_default_with_test_store();
    assert!(
        matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.png")),
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
///   1. s = load_default_test() → 默认壁纸
///   2. s.select_builtin("wallpapers/220240.gif") → 持久化 kind=builtin,value=wallpapers/220240.gif
///   3. s2 = load_default_test() → 重新打开 store 读取 → 还原 Builtin{220240.gif}
///
/// 预期结果：
///   - 正断言：s2.selection == Builtin{rel == "wallpapers/220240.gif"}
///   - 负断言：s2.selection 不应仍是默认 1331865.png
#[test]
fn test_builtin_selection_round_trips_through_persistence() {
    let _g = TestRootGuard::new();

    let mut s = WallpaperSettingsState::load_default_test();
    s.select_builtin("wallpapers/220240.gif");

    let s2 = WallpaperSettingsState::load_default_test();
    assert!(
        matches!(&s2.selection, WallpaperSelection::Builtin { rel } if rel == &"wallpapers/220240.gif"),
        "reloaded selection must be the gif builtin we just saved"
    );
    assert!(
        !matches!(&s2.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.png")),
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
///      → 回退默认 Builtin{1331865.png} 并持久化
///
/// 预期结果：
///   - 正断言：删后 selection 是 Builtin{ends_with("1331865.png")}
///   - 正断言：mine.png 文件已从磁盘移除
///   - 负断言：删后 selection 不应仍为 Custom
#[test]
fn test_delete_custom_falls_back_to_default_and_removes_file() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("mine.png"), b"fake").expect("write fake custom wallpaper");

    let mut s = WallpaperSettingsState::load_default_test();
    s.select_custom("mine.png".to_string());
    s.delete_custom("mine.png");

    assert!(
        matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.png")),
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
///   2. s2 = load_default_test() → 重读 → 还原 Custom{night.jpg}
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

    let mut s = WallpaperSettingsState::load_default_test();
    s.select_custom("night.jpg".to_string());

    let s2 = WallpaperSettingsState::load_default_test();
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

/// T-W5: resolve_path 与 image_asset_path 在不同 selection 下的语义。
///
/// 数据构造：
///   tempdir 作为 MULTISOUL_TEST_DATA_ROOT
///   1) 默认 Builtin{1331865.png}（Static）→ resolve_path 应解析到开发态 assets，image_asset_path = Some(rel)
///   2) Builtin{220240.gif}（Gif）→ image_asset_path = Some(rel)
///   3) Custom{mine.png} with real file → resolve_path = Some(path)；image_asset_path = None
///   4) Custom{missing.png} without file → resolve_path = None
///   5) Builtin{1403512.mp4}（Video）→ image_asset_path = None（视频不走 asset path）
///
/// 执行过程：分别构造 selection 并调用 resolve_path/image_asset_path。
///
/// 预期结果：
///   - 默认 Static builtin：image_asset_path == Some("wallpapers/1331865.png")
///   - Gif builtin：image_asset_path == Some("wallpapers/220240.gif")
///   - Video builtin：image_asset_path == None
///   - 存在的 custom：resolve_path 返回 Some 且 ends_with("mine.png")
///   - 缺失的 custom：resolve_path 返回 None
#[test]
fn test_resolve_path_and_image_asset_path_semantics() {
    let _g = TestRootGuard::new();

    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).expect("create custom wallpapers dir");
    fs::write(dir.join("mine.png"), b"fake").expect("write fake custom wallpaper");

    // Default Static builtin.
    let s_default = WallpaperSettingsState::load_default_with_test_store();
    assert_eq!(
        s_default.image_asset_path(),
        Some("wallpapers/1331865.png"),
        "default Static builtin must expose its image asset path"
    );
    assert!(
        s_default.resolve_path().is_some(),
        "default builtin must resolve to a real on-disk path in dev"
    );

    // Gif builtin.
    let mut s_gif = WallpaperSettingsState::load_default_test();
    s_gif.select_builtin("wallpapers/220240.gif");
    assert_eq!(
        s_gif.image_asset_path(),
        Some("wallpapers/220240.gif"),
        "Gif builtin must expose its image asset path"
    );

    // Video builtin: asset path is None (videos are not GPUI ImageSource).
    let mut s_video = WallpaperSettingsState::load_default_test();
    s_video.select_builtin("wallpapers/1403512.mp4");
    assert_eq!(
        s_video.current_kind(),
        crate::app::wallpaper_resolvers::WallpaperKind::Video,
        "the mp4 builtin must classify as Video"
    );
    assert_eq!(
        s_video.image_asset_path(),
        None,
        "video builtins must NOT expose an image asset path"
    );

    // Existing custom: resolve_path Some, image_asset_path None.
    let mut s_custom = WallpaperSettingsState::load_default_test();
    s_custom.select_custom("mine.png".to_string());
    let resolved = s_custom
        .resolve_path()
        .expect("existing custom file must resolve");
    assert!(
        resolved.to_string_lossy().ends_with("mine.png"),
        "custom resolve_path must end with the custom filename; got {resolved:?}"
    );
    assert_eq!(
        s_custom.image_asset_path(),
        None,
        "custom selection must NOT return an image asset path"
    );

    // Missing custom: resolve_path None.
    let mut s_missing = WallpaperSettingsState::load_default_test();
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

    let s = WallpaperSettingsState::load_default_with_test_store();
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

    let mut s = WallpaperSettingsState::load_default_test();
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
///   WallpaperSettingsState::load_default_test() → 默认 builtin 选择
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

    let mut s = WallpaperSettingsState::load_default_test();
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
///   load_default_test() → 默认 builtin
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

    let mut s = WallpaperSettingsState::load_default_test();
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

/// T-W9: wallpaper view 模块必须包含缩略图网格锚点、Import 按钮，且不依赖运行时。
///
/// 锚点：view.rs 源文件内必须出现
///   - "wallpaper-thumb"        （内置缩略格 cell id 前缀）
///   - "Import"                  （Import 按钮文案）
///   - "wallpaper-import"        （Import 按钮 id）
///   - "wallpaper-delete"        （custom cell 删除按钮 id 前缀）
///   - Task 11 视频占位注释
///
/// 这是结构性的源码锚点测试，避免 view 被误重构时丢失关键交互元素。
#[test]
fn test_wallpaper_view_has_grid_import_and_builtin_nodelete() {
    let src = include_str!("../../src/settings/wallpaper/view.rs");
    assert!(
        src.contains("wallpaper-thumb"),
        "view must define builtin thumbnail cell anchors (id \"wallpaper-thumb\")"
    );
    assert!(
        src.contains("Import"),
        "view must render an Import button label"
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
        src.contains("Task 11"),
        "video placeholder must reference the Task 11 first-frame follow-up"
    );
}
