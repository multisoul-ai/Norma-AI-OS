use super::*;

/// 旧版内置壁纸只由 bundle Resources 分发，不应重复嵌入主二进制。
///
/// 数据构造（含关键数值的推导过程）：
///   asset path = "wallpapers/1331865.webp"
///   WebP RIFF header = 12 bytes（RIFF + file size + WEBP）
///   packaged copy = Contents/Resources/wallpapers/1331865.webp
///   embedded copy = 0 bytes（避免与 packaged copy 重复）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. EmbeddedAssets.load("wallpapers/1331865.webp") → 返回 None
///   2. EmbeddedAssets.load("wallpapers/missing.png") → 返回 None
///
/// 预期结果：
///   - 正断言：旧版壁纸不由 EmbeddedAssets 返回
///   - 负断言：未知壁纸同样不应返回任何嵌入字节
#[test]
fn test_embedded_assets_do_not_duplicate_packaged_wallpaper_bytes() {
    let assets = EmbeddedAssets;
    let default = assets
        .load("wallpapers/1331865.webp")
        .expect("classic wallpaper lookup should not fail");
    let missing = assets
        .load("wallpapers/missing.png")
        .expect("unknown wallpaper lookup should not fail");

    assert!(
        default.is_none(),
        "packaged classic wallpaper must not also be embedded in the main binary"
    );
    assert!(
        missing.is_none(),
        "unknown wallpaper path must not return embedded bytes"
    );
}

/// 内置 App 图标资产：workspace 启动页 logo 应读取项目内圆角 PNG，而不是临时文件或代码绘制占位符。
///
/// 数据构造（含关键数值的推导过程）：
///   asset path = "icons/app-icon.png"
///   PNG signature = 8 bytes（89 50 4E 47 0D 0A 1A 0A）
///   source bytes = include_bytes!("../assets/icons/app-icon.png")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. EmbeddedAssets.load(APP_ICON_ASSET) → 返回 Some(bytes)
///   2. 检查 bytes 前 8 字节 → 匹配 PNG signature
///   3. EmbeddedAssets.load("icons/missing.png") → 返回 None
///
/// 预期结果：
///   - 正断言：App icon asset 存在且字节非空
///   - 正断言：App icon asset 是 PNG
///   - 负断言：未知 icon 路径不应返回 App icon 字节
#[test]
fn test_embedded_assets_load_app_icon_png() {
    let assets = EmbeddedAssets;
    let loaded = assets
        .load(APP_ICON_ASSET)
        .expect("embedded app icon lookup should not fail")
        .expect("app icon asset should be present");

    assert!(!loaded.is_empty(), "embedded app icon should contain bytes");
    assert_eq!(
        &loaded[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "embedded app icon should be a PNG file"
    );
    assert!(
        assets
            .load("icons/missing.png")
            .expect("unknown icon lookup should not fail")
            .is_none(),
        "unknown icon path must not return the app icon bytes"
    );
}

/// 内置资产目录：图标和首页背景写入主二进制，壁纸由 bundle Resources 分发。
///
/// 数据构造（含关键数值的推导过程）：
///   root = "" → 2 个顶层目录（icons、home）
///   wallpapers embedded entries = 0（11 张图片均只保留 bundle copy）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. list root/icons/home/wallpapers
///   2. 校验 root 暴露 icons、home，icons 非空、home 恰有固定背景，wallpapers 为空
///
/// 预期结果：
///   - 正断言：root 精确包含 icons 和 home，且两个目录仍可枚举
///   - 负断言：root 不包含 wallpapers，wallpapers 不返回任何嵌入项
#[test]
fn test_embedded_assets_list_only_binary_assets() {
    let assets = EmbeddedAssets;
    let root = assets
        .list("")
        .expect("root embedded asset listing should not fail");
    let icons = assets
        .list("icons")
        .expect("icon embedded asset listing should not fail");
    let home = assets
        .list("home")
        .expect("home embedded asset listing should not fail");
    let wallpapers = assets
        .list("wallpapers")
        .expect("wallpaper embedded asset listing should not fail");

    assert_eq!(
        root,
        vec![SharedString::from("icons"), SharedString::from("home")],
        "root asset listing should expose only actually embedded directories"
    );
    assert!(
        !icons.is_empty(),
        "embedded icons directory must remain available after wallpaper deduplication"
    );
    assert_eq!(
        home,
        vec![SharedString::from("earth-horizon.png")],
        "embedded home directory should contain only the selected fixed background"
    );
    assert!(
        wallpapers.is_empty(),
        "packaged wallpapers must not appear in the embedded asset listing"
    );
    assert!(
        !root.iter().any(|entry| entry.as_ref() == "wallpapers"),
        "embedded asset root must not advertise a wallpapers directory"
    );
}

/// 顶栏 SVG 图标资产：每个 icons/toolbar-*.svg 路径都应能通过 AssetSource 加载。
///
/// 数据构造（含关键数值的推导过程）：
///   toolbar 路径数 = 12 + 1 = 13；首非空白字节期望 = b'<'（ASCII 60）；
///   负样本路径 = "icons/toolbar-missing.svg"。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐一 load 13 条 toolbar 路径并检查首非空白字节
///   2. load 缺失路径并期望 None
///
/// 预期结果：
///   - 正断言：13 条路径均返回 Some 且首非空白字节为 b'<'
///   - 负断言：不存在的 toolbar-missing.svg 返回 None
#[test]
fn test_embedded_assets_load_toolbar_svg_icons() {
    let assets = EmbeddedAssets;

    let toolbar_paths = [
        TOOLBAR_LAYOUT_ASSET,
        TOOLBAR_SCROLL_ASSET,
        TOOLBAR_PLUS_ASSET,
        TOOLBAR_TERMINAL_ASSET,
        TOOLBAR_BROWSER_ASSET,
        TOOLBAR_CLAUDE_ASSET,
        TOOLBAR_CODEX_ASSET,
        TOOLBAR_CURSOR_ASSET,
        TOOLBAR_GROK_ASSET,
        TOOLBAR_OPENCODE_ASSET,
        TOOLBAR_KIMI_ASSET,
        TOOLBAR_ZCODE_ASSET,
        TOOLBAR_PI_ASSET,
        TOOLBAR_HERMES_ASSET,
        TOOLBAR_PREVIEW_ASSET,
        TOOLBAR_SETTINGS_ASSET,
        TOOLBAR_WORKSPACES_ASSET,
    ];

    for path in toolbar_paths.iter() {
        let loaded = assets
            .load(path)
            .expect("toolbar svg asset lookup should not fail")
            .unwrap_or_else(|| panic!("toolbar svg asset {:?} should be present", path));

        assert!(
            !loaded.is_empty(),
            "toolbar svg asset {:?} should contain bytes",
            path
        );

        // SVG 文件以 `<` 开头（可能带前导空白）；找到首非空白字节断言为 b'<'。
        let first_non_ws = loaded
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .expect("toolbar svg asset should have at least one non-whitespace byte");
        assert_eq!(
            loaded[first_non_ws], b'<',
            "toolbar svg asset {:?} must start with '<' (SVG markup), got byte {:#04x}",
            path, loaded[first_non_ws]
        );
    }

    assert!(
        assets
            .load("icons/toolbar-missing.svg")
            .expect("missing toolbar svg lookup should not fail")
            .is_none(),
        "non-existent toolbar svg path must not resolve to any bytes"
    );
}
