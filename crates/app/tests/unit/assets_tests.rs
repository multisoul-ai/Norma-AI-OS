use super::*;

/// 内置产品 Logo 资产：workspace 启动页品牌图应读取独立 logo PNG，而不是复用 App icon。
///
/// 数据构造（含关键数值的推导过程）：
///   logo asset path = APP_LOGO_ASSET = "icons/logo.png"
///   icon asset path = "icons/app-icon.png"
///   PNG signature   = 8 bytes（89 50 4E 47 0D 0A 1A 0A）
///   unknown path    = "icons/logo-missing.png"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. EmbeddedAssets.load(APP_LOGO_ASSET) → 应返回 Some(bytes)
///   2. EmbeddedAssets.load(APP_ICON_ASSET) → 应返回 Some(icon_bytes)
///   3. 检查 logo bytes 前 8 字节 → 匹配 PNG signature
///   4. 比较 APP_LOGO_ASSET 与 APP_ICON_ASSET → 两者路径不应相同
///   5. EmbeddedAssets.load("icons/logo-missing.png") → 返回 None
///
/// 预期结果：
///   - 正断言：Logo asset 存在且字节非空
///   - 正断言：Logo asset 是 PNG
///   - 负断言：Logo asset path 不应与 App icon asset path 相同
///   - 负断言：未知 logo 路径不应返回 Logo 字节
#[test]
fn test_embedded_assets_load_app_logo_png() {
    let assets = EmbeddedAssets;
    let logo = assets
        .load(APP_LOGO_ASSET)
        .expect("embedded app logo lookup should not fail")
        .expect("app logo asset should be present");
    let icon = assets
        .load(APP_ICON_ASSET)
        .expect("embedded app icon lookup should not fail")
        .expect("app icon asset should be present");

    assert!(!logo.is_empty(), "embedded app logo should contain bytes");
    assert_eq!(
        &logo[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "embedded app logo should be a PNG file"
    );
    assert_ne!(
        APP_LOGO_ASSET, APP_ICON_ASSET,
        "embedded app logo must have a separate asset path from the app icon"
    );
    assert!(
        !icon.is_empty(),
        "embedded app icon should still load bytes"
    );
    assert!(
        assets
            .load("icons/logo-missing.png")
            .expect("unknown logo lookup should not fail")
            .is_none(),
        "unknown logo path must not return the app logo bytes"
    );
}

/// Themes 导航图标：导航引用的 SVG 必须由 EmbeddedAssets 加载并出现在图标目录。
///
/// 数据构造（含关键数值的推导过程）：
///   visible Themes icons = 1
///   asset path = "icons/settings-nav-themes.svg"
///   SVG prefix = "<svg"（文本矢量资源，不是 PNG/JPEG）
///   unknown path = "icons/settings-nav-themes-missing.svg"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. EmbeddedAssets.load(Themes path) → 应返回 Some(bytes)。
///   2. 将 bytes 解码为 UTF-8，确认包含 SVG 根元素。
///   3. EmbeddedAssets.list("icons") → 应包含 themes 文件名。
///   4. 查询 unknown path → 应返回 None，防止未知路径误命中 Themes 字节。
///
/// 预期结果：
///   - 正断言：Themes SVG 存在、可解码且被 icons 目录枚举。
///   - 负断言：Themes SVG 不得为空，也不得由未知路径返回。
#[test]
fn test_embedded_assets_load_themes_navigation_svg() {
    let assets = EmbeddedAssets;
    let themes_path = "icons/settings-nav-themes.svg";
    let themes = assets
        .load(themes_path)
        .expect("Themes navigation icon lookup should not fail")
        .expect("Themes navigation icon must be embedded");
    let svg = std::str::from_utf8(&themes).expect("Themes navigation icon must be valid UTF-8 SVG");
    let icons = assets
        .list("icons")
        .expect("embedded icon directory listing should not fail");

    assert!(
        !themes.is_empty(),
        "Themes navigation SVG must contain bytes"
    );
    assert!(
        svg.contains("<svg"),
        "Themes navigation asset must contain an SVG root element"
    );
    assert!(
        icons
            .iter()
            .any(|name| name.as_ref() == "settings-nav-themes.svg"),
        "icons directory must enumerate settings-nav-themes.svg"
    );
    assert!(
        assets
            .load("icons/settings-nav-themes-missing.svg")
            .expect("unknown Themes icon lookup should not fail")
            .is_none(),
        "unknown Themes icon path must not return embedded SVG bytes"
    );
}
