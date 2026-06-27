//! Embedded GPUI assets that should travel with the application binary.

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub const APP_ICON_ASSET: &str = "icons/app-icon.png";
pub const DEFAULT_WALLPAPER_ASSET: &str = "wallpapers/1331865.png";

/// Toolbar SVG icon asset paths. These mirror `crate::ui::icons::ToolbarIcon::asset_path`
/// exactly (icons/toolbar-<name>.svg) so GPUI `svg().path(...)` can resolve them.
pub const TOOLBAR_SCROLL_ASSET: &str = "icons/toolbar-scroll.svg";
pub const TOOLBAR_PLUS_ASSET: &str = "icons/toolbar-plus.svg";
pub const TOOLBAR_TERMINAL_ASSET: &str = "icons/toolbar-terminal.svg";
pub const TOOLBAR_CLAUDE_ASSET: &str = "icons/toolbar-claude.svg";
pub const TOOLBAR_CODEX_ASSET: &str = "icons/toolbar-codex.svg";
pub const TOOLBAR_PREVIEW_ASSET: &str = "icons/toolbar-preview.svg";
pub const TOOLBAR_SETTINGS_ASSET: &str = "icons/toolbar-settings.svg";

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/app-icon.png");
const DEFAULT_WALLPAPER_BYTES: &[u8] = include_bytes!("../assets/wallpapers/1331865.png");

const TOOLBAR_SCROLL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-scroll.svg");
const TOOLBAR_PLUS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-plus.svg");
const TOOLBAR_TERMINAL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-terminal.svg");
const TOOLBAR_CLAUDE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-claude.svg");
const TOOLBAR_CODEX_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-codex.svg");
const TOOLBAR_PREVIEW_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-preview.svg");
const TOOLBAR_SETTINGS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-settings.svg");

#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path {
            APP_ICON_ASSET => Ok(Some(Cow::Borrowed(APP_ICON_BYTES))),
            DEFAULT_WALLPAPER_ASSET => Ok(Some(Cow::Borrowed(DEFAULT_WALLPAPER_BYTES))),
            TOOLBAR_SCROLL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SCROLL_BYTES))),
            TOOLBAR_PLUS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PLUS_BYTES))),
            TOOLBAR_TERMINAL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_TERMINAL_BYTES))),
            TOOLBAR_CLAUDE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CLAUDE_BYTES))),
            TOOLBAR_CODEX_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CODEX_BYTES))),
            TOOLBAR_PREVIEW_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PREVIEW_BYTES))),
            TOOLBAR_SETTINGS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SETTINGS_BYTES))),
            _ => Ok(None),
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
                SharedString::from("toolbar-scroll.svg"),
                SharedString::from("toolbar-plus.svg"),
                SharedString::from("toolbar-terminal.svg"),
                SharedString::from("toolbar-claude.svg"),
                SharedString::from("toolbar-codex.svg"),
                SharedString::from("toolbar-preview.svg"),
                SharedString::from("toolbar-settings.svg"),
            ]),
            "wallpapers" => Ok(vec![SharedString::from("1331865.png")]),
            _ => Ok(vec![]),
        }
    }
}

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
    ///   root list path = "" → 2 个顶层目录 icons、wallpapers
    ///   icons list path = "icons" → 8 个文件（1 PNG + 7 SVG）
    ///     1. app-icon.png
    ///     2. toolbar-scroll.svg
    ///     3. toolbar-plus.svg
    ///     4. toolbar-terminal.svg
    ///     5. toolbar-claude.svg
    ///     6. toolbar-codex.svg
    ///     7. toolbar-preview.svg
    ///     8. toolbar-settings.svg
    ///   wallpapers list path = "wallpapers" → 1 个文件 1331865.png
    ///   unknown list path = "missing" → 0 个条目
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. list("") → 返回 icons、wallpapers
    ///   2. list("icons") → 返回 app-icon.png + 7 个 toolbar-*.svg（顺序与 icons.rs 一致）
    ///   3. list("wallpapers") → 返回 1331865.png
    ///   4. list("missing") → 返回空列表
    ///
    /// 预期结果：
    ///   - 正断言：root 包含 icons 与 wallpapers
    ///   - 正断言：icons 顺序精确等于 [app-icon.png, toolbar-scroll.svg, ..., toolbar-settings.svg]
    ///   - 正断言：wallpapers 包含 1331865.png
    ///   - 负断言：missing 不包含任何条目
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
                SharedString::from("toolbar-scroll.svg"),
                SharedString::from("toolbar-plus.svg"),
                SharedString::from("toolbar-terminal.svg"),
                SharedString::from("toolbar-claude.svg"),
                SharedString::from("toolbar-codex.svg"),
                SharedString::from("toolbar-preview.svg"),
                SharedString::from("toolbar-settings.svg"),
            ],
            "icons asset listing should expose the bundled app icon plus the 7 toolbar SVGs in icons.rs order"
        );
        assert_eq!(
            wallpapers,
            vec![SharedString::from("1331865.png")],
            "wallpapers asset listing should expose the bundled default image"
        );
        assert!(
            missing.is_empty(),
            "unknown embedded asset directory should list no entries"
        );
    }

    /// 顶栏 SVG 图标资产：每个 icons/toolbar-*.svg 路径都应能通过 AssetSource 加载，
    /// 返回非空字节且首字节为 `<`（SVG 文件以 `<svg` 或 `<?xml` 开头）。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   toolbar 路径表（与 crates/ui/src/icons.rs::ToolbarIcon::asset_path 完全一致）：
    ///     icons/toolbar-scroll.svg
    ///     icons/toolbar-plus.svg
    ///     icons/toolbar-terminal.svg
    ///     icons/toolbar-claude.svg
    ///     icons/toolbar-codex.svg
    ///     icons/toolbar-preview.svg
    ///     icons/toolbar-settings.svg
    ///   路径数 = 7
    ///   首非空白字节期望 = b'<'（ASCII 60），SVG 文本必然以 `<` 起始
    ///   负样本路径 = "icons/toolbar-missing.svg"（不存在）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 对 7 条路径逐一调用 EmbeddedAssets.load(path)
    ///   2. 每条期望返回 Some(bytes)
    ///   3. 对 bytes 跳过前导 ASCII 空白，取首非空白字节，断言 == b'<'
    ///   4. 调用 load("icons/toolbar-missing.svg") → 期望 None
    ///
    /// 预期结果：
    ///   - 正断言：7 条路径均返回 Some 且字节非空
    ///   - 正断言：7 条路径首非空白字节均为 b'<'（SVG 起始标记）
    ///   - 负断言：不存在的 toolbar-missing.svg 返回 None
    #[test]
    fn test_embedded_assets_load_toolbar_svg_icons() {
        let assets = EmbeddedAssets;

        let toolbar_paths = [
            TOOLBAR_SCROLL_ASSET,
            TOOLBAR_PLUS_ASSET,
            TOOLBAR_TERMINAL_ASSET,
            TOOLBAR_CLAUDE_ASSET,
            TOOLBAR_CODEX_ASSET,
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
