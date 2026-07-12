//! Embedded GPUI assets that should travel with the application binary.

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub const APP_ICON_ASSET: &str = "icons/app-icon.png";
pub const APP_LOGO_ASSET: &str = "icons/logo.png";
pub const DEFAULT_WALLPAPER_ASSET: &str = "wallpapers/1331865.png";

/// Toolbar SVG icon asset paths. These mirror `crate::ui::icons::ToolbarIcon::asset_path`
/// exactly (icons/toolbar-<name>.svg) so GPUI `svg().path(...)` can resolve them.
pub const TOOLBAR_SCROLL_ASSET: &str = "icons/toolbar-scroll.svg";
pub const TOOLBAR_PLUS_ASSET: &str = "icons/toolbar-plus.svg";
pub const TOOLBAR_TERMINAL_ASSET: &str = "icons/toolbar-terminal.svg";
pub const TOOLBAR_BROWSER_ASSET: &str = "icons/toolbar-browser.svg";
pub const TOOLBAR_CLAUDE_ASSET: &str = "icons/toolbar-claude.svg";
pub const TOOLBAR_CODEX_ASSET: &str = "icons/toolbar-codex.svg";
pub const TOOLBAR_OPENCODE_ASSET: &str = "icons/toolbar-opencode.svg";
pub const TOOLBAR_PREVIEW_ASSET: &str = "icons/toolbar-preview.svg";
pub const TOOLBAR_SETTINGS_ASSET: &str = "icons/toolbar-settings.svg";

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/app-icon.png");
const APP_LOGO_BYTES: &[u8] = include_bytes!("../assets/icons/logo.png");

/// 为所有内置 image 壁纸（3 PNG + 2 GIF）生成 `include_bytes!` 常量与按相对路径
/// 查询的 `image_builtin_bytes` 辅助函数。
///
/// `include_bytes!(concat!("../assets/", $rel))` 在编译期把字面量拼成
/// `../assets/wallpapers/<name>.{png|gif}`，相对 `crates/app/src/assets.rs` 解析。
/// 视频不在其中：MP4 文件较大，走 Task 9 的 git-lfs + bundle Resources 流程，
/// 不打进二进制。
macro_rules! image_builtins {
    ( $( ($rel:literal, $const_name:ident) ),* $(,)? ) => {
        $( const $const_name: &[u8] = include_bytes!(concat!("../assets/", $rel)); )*
        fn image_builtin_bytes(path: &str) -> Option<Cow<'static, [u8]>> {
            match path {
                $( $rel => Some(Cow::Borrowed($const_name)), )*
                _ => None,
            }
        }
    };
}
image_builtins! {
    ("wallpapers/1331865.png", DEFAULT_WALLPAPER_BYTES),
    ("wallpapers/1350954.png", WALLPAPER_1350954_BYTES),
    ("wallpapers/1358524.png", WALLPAPER_1358524_BYTES),
    ("wallpapers/215824.gif",  WALLPAPER_215824_BYTES),
    ("wallpapers/220240.gif",  WALLPAPER_220240_BYTES),
}

const TOOLBAR_SCROLL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-scroll.svg");
const TOOLBAR_PLUS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-plus.svg");
const TOOLBAR_TERMINAL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-terminal.svg");
const TOOLBAR_BROWSER_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-browser.svg");
const TOOLBAR_CLAUDE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-claude.svg");
const TOOLBAR_CODEX_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-codex.svg");
const TOOLBAR_OPENCODE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-opencode.svg");
const TOOLBAR_PREVIEW_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-preview.svg");
const TOOLBAR_SETTINGS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-settings.svg");

#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path {
            APP_ICON_ASSET => Ok(Some(Cow::Borrowed(APP_ICON_BYTES))),
            APP_LOGO_ASSET => Ok(Some(Cow::Borrowed(APP_LOGO_BYTES))),
            TOOLBAR_SCROLL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SCROLL_BYTES))),
            TOOLBAR_PLUS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PLUS_BYTES))),
            TOOLBAR_TERMINAL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_TERMINAL_BYTES))),
            TOOLBAR_BROWSER_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_BROWSER_BYTES))),
            TOOLBAR_CLAUDE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CLAUDE_BYTES))),
            TOOLBAR_CODEX_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CODEX_BYTES))),
            TOOLBAR_OPENCODE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_OPENCODE_BYTES))),
            TOOLBAR_PREVIEW_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PREVIEW_BYTES))),
            TOOLBAR_SETTINGS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SETTINGS_BYTES))),
            _ => match image_builtin_bytes(path) {
                Some(bytes) => Ok(Some(bytes)),
                None => Ok(None),
            },
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        match path {
            "" => Ok(vec![
                SharedString::from("icons"),
                SharedString::from("wallpapers"),
            ]),
            "icons" => Ok(vec![
                SharedString::from("app-icon.png"),
                SharedString::from("logo.png"),
                SharedString::from("toolbar-scroll.svg"),
                SharedString::from("toolbar-plus.svg"),
                SharedString::from("toolbar-terminal.svg"),
                SharedString::from("toolbar-browser.svg"),
                SharedString::from("toolbar-claude.svg"),
                SharedString::from("toolbar-codex.svg"),
                SharedString::from("toolbar-opencode.svg"),
                SharedString::from("toolbar-preview.svg"),
                SharedString::from("toolbar-settings.svg"),
            ]),
            // 仅枚举 image 内置项（PNG/GIF）。视频文件不在 EmbeddedAssets 分发，
            // 由 builtin_wallpapers() 过滤掉 Video kind 后派生，避免与清单脱钩。
            "wallpapers" => Ok(crate::app::wallpaper_resolvers::builtin_wallpapers()
                .iter()
                .filter(|(_, _, kind)| {
                    !matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video)
                })
                .map(|(rel, _, _)| {
                    SharedString::from(
                        rel.rsplit('/')
                            .next()
                            .expect("builtin wallpaper rel path must have a filename"),
                    )
                })
                .collect()),
            _ => Ok(vec![]),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/assets_tests.rs"]
mod assets_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 内置壁纸资产：默认壁纸应由 include_bytes 打进程序并能通过 GPUI AssetSource 读取。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   asset path = "wallpapers/1331865.png"
    ///   PNG signature = 8 bytes（89 50 4E 47 0D 0A 1A 0A）
    ///   source bytes = include_bytes!("../assets/wallpapers/1331865.png")
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. EmbeddedAssets.load(DEFAULT_WALLPAPER_ASSET) → 返回 Some(bytes)
    ///   2. 检查 bytes 前 8 字节 → 匹配 PNG signature
    ///   3. EmbeddedAssets.load("wallpapers/missing.png") → 返回 None
    ///
    /// 预期结果：
    ///   - 正断言：默认 asset 存在且字节非空
    ///   - 正断言：默认 asset 以 PNG signature 开头
    ///   - 负断言：未知 asset 不应返回默认壁纸字节
    #[test]
    fn test_embedded_assets_load_default_wallpaper_png() {
        let assets = EmbeddedAssets;
        let loaded = assets
            .load(DEFAULT_WALLPAPER_ASSET)
            .expect("embedded asset source should not fail")
            .expect("default wallpaper asset should be present");

        assert!(
            !loaded.is_empty(),
            "embedded default wallpaper should contain bytes"
        );
        assert_eq!(
            &loaded[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
            "embedded default wallpaper should be a PNG file"
        );
        assert!(
            assets
                .load("wallpapers/missing.png")
                .expect("unknown asset lookup should not fail")
                .is_none(),
            "unknown asset path must not return the default wallpaper bytes"
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

    /// 内置资产目录：GPUI 查询 icons 与 wallpapers 目录时应能枚举对应文件名。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   root = "" → 2 个顶层目录；icons → 11 个文件（2 PNG + 9 SVG）；
    ///   wallpapers → 5 个 image built-in（3 PNG + 2 GIF），不含视频。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. list root/icons/wallpapers/missing
    ///   2. 校验 icons 精确顺序、wallpapers image 集合和 missing 空集合
    ///
    /// 预期结果：
    ///   - 正断言：root、icons、wallpapers 均返回预期内容
    ///   - 负断言：wallpapers 不包含视频文件，missing 不包含任何条目
    #[test]
    fn test_embedded_assets_list_wallpaper_directory() {
        let assets = EmbeddedAssets;

        let root = assets
            .list("")
            .expect("root embedded asset listing should not fail");
        let icons = assets
            .list("icons")
            .expect("icon embedded asset listing should not fail");
        let wallpapers = assets
            .list("wallpapers")
            .expect("wallpaper embedded asset listing should not fail");
        let missing = assets
            .list("missing")
            .expect("unknown embedded asset listing should not fail");

        assert_eq!(
            root,
            vec![
                SharedString::from("icons"),
                SharedString::from("wallpapers")
            ],
            "root asset listing should expose the icons and wallpapers directories"
        );
        assert_eq!(
            icons,
            vec![
                SharedString::from("app-icon.png"),
                SharedString::from("logo.png"),
                SharedString::from("toolbar-scroll.svg"),
                SharedString::from("toolbar-plus.svg"),
                SharedString::from("toolbar-terminal.svg"),
                SharedString::from("toolbar-browser.svg"),
                SharedString::from("toolbar-claude.svg"),
                SharedString::from("toolbar-codex.svg"),
                SharedString::from("toolbar-opencode.svg"),
                SharedString::from("toolbar-preview.svg"),
                SharedString::from("toolbar-settings.svg"),
            ],
            "icons asset listing should expose the app icon, logo, and 9 toolbar SVGs in icons.rs order"
        );
        // 期望的 image built-in 文件名集合（由 builtin_wallpapers() 过滤掉 Video 派生）。
        let expected_image_filenames: Vec<&str> =
            crate::app::wallpaper_resolvers::builtin_wallpapers()
                .iter()
                .filter(|(_, _, kind)| {
                    !matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video)
                })
                .map(|(rel, _, _)| {
                    rel.rsplit('/')
                        .next()
                        .expect("builtin rel path has a filename")
                })
                .collect();
        assert_eq!(
            expected_image_filenames.len(),
            5,
            "there should be exactly 5 image built-ins (3 PNG + 2 GIF): got {:?}",
            expected_image_filenames
        );
        for fname in &expected_image_filenames {
            assert!(
                wallpapers.iter().any(|s| s.as_ref() == *fname),
                "wallpapers listing must include image built-in {:?}; got {:?}",
                fname,
                wallpapers
            );
        }
        // 负断言：视频文件名绝不应出现在 EmbeddedAssets 的 wallpapers 枚举中（视频不经此通道分发）。
        for (rel, _, kind) in crate::app::wallpaper_resolvers::builtin_wallpapers() {
            if matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video) {
                let vfname = rel
                    .rsplit('/')
                    .next()
                    .expect("video rel path has a filename");
                assert!(
                    !wallpapers.iter().any(|s| s.as_ref() == vfname),
                    "video built-in {:?} must NOT be served by EmbeddedAssets (Task 9 territory); got {:?}",
                    vfname,
                    wallpapers
                );
            }
        }

        assert!(
            missing.is_empty(),
            "unknown embedded asset directory should list no entries"
        );
    }

    /// 内置 image 资产：EmbeddedAssets.load/list 应覆盖所有 Static 与 GIF 内置项（不含视频）。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   builtin_wallpapers() = 10 项 → 5 项 image（3 PNG + 2 GIF）+ 5 项 Video。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 对每个 image builtin 调 EmbeddedAssets.load(rel)
    ///   2. 校验 PNG/GIF magic，并确认 list("wallpapers") 包含文件名
    ///
    /// 预期结果：
    ///   - 正断言：5 个 image builtin 均能 load 且 magic 正确
    ///   - 正断言：list("wallpapers") 包含全部 5 个 image 文件名
    #[test]
    fn test_embedded_assets_serve_all_image_builtins() {
        let assets = EmbeddedAssets;

        for (rel, _name, kind) in crate::app::wallpaper_resolvers::builtin_wallpapers() {
            if matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video) {
                continue;
            }
            let bytes = assets
                .load(rel)
                .expect("image builtin load must not error")
                .unwrap_or_else(|| panic!("image builtin {rel} must be embedded"));
            assert!(
                !bytes.is_empty(),
                "builtin {rel} embedded bytes must be non-empty"
            );

            match kind {
                crate::app::wallpaper_resolvers::WallpaperKind::Static => {
                    assert_eq!(
                        &bytes[..8],
                        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
                        "static builtin {rel} must start with PNG signature"
                    );
                }
                crate::app::wallpaper_resolvers::WallpaperKind::Gif => {
                    assert_eq!(
                        &bytes[..4],
                        b"GIF8",
                        "gif builtin {rel} must start with GIF8 magic"
                    );
                }
                crate::app::wallpaper_resolvers::WallpaperKind::Video => {
                    unreachable!("video built-ins are filtered out above")
                }
            }
        }

        let listed = assets
            .list("wallpapers")
            .expect("list wallpapers must not error");
        for (rel, _, kind) in crate::app::wallpaper_resolvers::builtin_wallpapers() {
            if matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video) {
                continue;
            }
            let fname = rel.rsplit('/').next().expect("path has filename");
            assert!(
                listed.iter().any(|s| s.as_ref() == fname),
                "wallpapers listing must include {fname}; got {:?}",
                listed
            );
        }
    }

    /// 顶栏 SVG 图标资产：每个 icons/toolbar-*.svg 路径都应能通过 AssetSource 加载。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   toolbar 路径数 = 9；首非空白字节期望 = b'<'（ASCII 60）；
    ///   负样本路径 = "icons/toolbar-missing.svg"。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 逐一 load 9 条 toolbar 路径并检查首非空白字节
    ///   2. load 缺失路径并期望 None
    ///
    /// 预期结果：
    ///   - 正断言：9 条路径均返回 Some 且首非空白字节为 b'<'
    ///   - 负断言：不存在的 toolbar-missing.svg 返回 None
    #[test]
    fn test_embedded_assets_load_toolbar_svg_icons() {
        let assets = EmbeddedAssets;

        let toolbar_paths = [
            TOOLBAR_SCROLL_ASSET,
            TOOLBAR_PLUS_ASSET,
            TOOLBAR_TERMINAL_ASSET,
            TOOLBAR_BROWSER_ASSET,
            TOOLBAR_CLAUDE_ASSET,
            TOOLBAR_CODEX_ASSET,
            TOOLBAR_OPENCODE_ASSET,
            TOOLBAR_PREVIEW_ASSET,
            TOOLBAR_SETTINGS_ASSET,
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
}
