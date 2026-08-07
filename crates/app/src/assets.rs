//! Embedded GPUI assets that should travel with the application binary.

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub const APP_ICON_ASSET: &str = "icons/app-icon.png";
pub const APP_LOGO_ASSET: &str = "icons/logo.png";
pub const DEFAULT_WALLPAPER_ASSET: &str = "wallpapers/1331865.webp";
/// Toolbar SVG icon asset paths. These mirror `crate::ui::icons::ToolbarIcon::asset_path`
/// exactly (icons/toolbar-<name>.svg) so GPUI `svg().path(...)` can resolve them.
pub const TOOLBAR_SCROLL_ASSET: &str = "icons/toolbar-scroll.svg";
pub const TOOLBAR_LAYOUT_ASSET: &str = "icons/toolbar-layout.svg";
pub const TOOLBAR_PLUS_ASSET: &str = "icons/toolbar-plus.svg";
pub const TOOLBAR_TERMINAL_ASSET: &str = "icons/toolbar-terminal.svg";
pub const TOOLBAR_BROWSER_ASSET: &str = "icons/toolbar-browser.svg";
pub const TOOLBAR_CLAUDE_ASSET: &str = "icons/toolbar-claude.svg";
pub const TOOLBAR_CODEX_ASSET: &str = "icons/toolbar-codex.svg";
pub const TOOLBAR_CURSOR_ASSET: &str = "icons/toolbar-cursor.svg";
pub const TOOLBAR_GROK_ASSET: &str = "icons/toolbar-grok.svg";
pub const TOOLBAR_OPENCODE_ASSET: &str = "icons/toolbar-opencode.svg";
pub const TOOLBAR_PREVIEW_ASSET: &str = "icons/toolbar-preview.svg";
pub const TOOLBAR_SETTINGS_ASSET: &str = "icons/toolbar-settings.svg";
pub const SETTINGS_NAV_THEMES_ASSET: &str = "icons/settings-nav-themes.svg";
pub const SETTINGS_NAV_WALLPAPER_ASSET: &str = "icons/settings-nav-wallpaper.svg";
pub const SETTINGS_NAV_KEYBOARD_ASSET: &str = "icons/settings-nav-keyboard.svg";
pub const SETTINGS_NAV_VOICE_ASSET: &str = "icons/settings-nav-voice.svg";
pub const SETTINGS_NAV_AGENTS_ASSET: &str = "icons/settings-nav-agents.svg";
pub const SETTINGS_NAV_HOOKS_ASSET: &str = "icons/settings-nav-hooks.svg";
pub const SETTINGS_CHECK_UPDATES_ASSET: &str = "icons/settings-check-updates.svg";
const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/app-icon.png");
const APP_LOGO_BYTES: &[u8] = include_bytes!("../assets/icons/logo.png");

const TOOLBAR_SCROLL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-scroll.svg");
const TOOLBAR_LAYOUT_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-layout.svg");
const TOOLBAR_PLUS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-plus.svg");
const TOOLBAR_TERMINAL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-terminal.svg");
const TOOLBAR_BROWSER_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-browser.svg");
const TOOLBAR_CLAUDE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-claude.svg");
const TOOLBAR_CODEX_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-codex.svg");
const TOOLBAR_CURSOR_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-cursor.svg");
const TOOLBAR_GROK_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-grok.svg");
const TOOLBAR_OPENCODE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-opencode.svg");
const TOOLBAR_PREVIEW_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-preview.svg");
const TOOLBAR_SETTINGS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-settings.svg");
const SETTINGS_NAV_THEMES_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-themes.svg");
const SETTINGS_NAV_WALLPAPER_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-nav-wallpaper.svg");
const SETTINGS_NAV_KEYBOARD_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-nav-keyboard.svg");
const SETTINGS_NAV_VOICE_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-voice.svg");
const SETTINGS_NAV_AGENTS_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-agents.svg");
const SETTINGS_NAV_HOOKS_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-hooks.svg");
const SETTINGS_CHECK_UPDATES_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-check-updates.svg");
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedAssets;
impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path {
            APP_ICON_ASSET => Ok(Some(Cow::Borrowed(APP_ICON_BYTES))),
            APP_LOGO_ASSET => Ok(Some(Cow::Borrowed(APP_LOGO_BYTES))),
            TOOLBAR_SCROLL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SCROLL_BYTES))),
            TOOLBAR_LAYOUT_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_LAYOUT_BYTES))),
            TOOLBAR_PLUS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PLUS_BYTES))),
            TOOLBAR_TERMINAL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_TERMINAL_BYTES))),
            TOOLBAR_BROWSER_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_BROWSER_BYTES))),
            TOOLBAR_CLAUDE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CLAUDE_BYTES))),
            TOOLBAR_CODEX_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CODEX_BYTES))),
            TOOLBAR_CURSOR_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CURSOR_BYTES))),
            TOOLBAR_GROK_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_GROK_BYTES))),
            TOOLBAR_OPENCODE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_OPENCODE_BYTES))),
            TOOLBAR_PREVIEW_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PREVIEW_BYTES))),
            TOOLBAR_SETTINGS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SETTINGS_BYTES))),
            SETTINGS_NAV_THEMES_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_THEMES_BYTES))),
            SETTINGS_NAV_WALLPAPER_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_WALLPAPER_BYTES))),
            SETTINGS_NAV_KEYBOARD_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_KEYBOARD_BYTES))),
            SETTINGS_NAV_VOICE_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_VOICE_BYTES))),
            SETTINGS_NAV_AGENTS_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_AGENTS_BYTES))),
            SETTINGS_NAV_HOOKS_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_HOOKS_BYTES))),
            SETTINGS_CHECK_UPDATES_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_CHECK_UPDATES_BYTES))),
            _ => Ok(None),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        match path {
            "" => Ok(vec![SharedString::from("icons")]),
            "icons" => Ok(vec![
                SharedString::from("app-icon.png"),
                SharedString::from("logo.png"),
                SharedString::from("toolbar-scroll.svg"),
                SharedString::from("toolbar-layout.svg"),
                SharedString::from("toolbar-plus.svg"),
                SharedString::from("toolbar-terminal.svg"),
                SharedString::from("toolbar-browser.svg"),
                SharedString::from("toolbar-claude.svg"),
                SharedString::from("toolbar-codex.svg"),
                SharedString::from("toolbar-cursor.svg"),
                SharedString::from("toolbar-grok.svg"),
                SharedString::from("toolbar-opencode.svg"),
                SharedString::from("toolbar-preview.svg"),
                SharedString::from("toolbar-settings.svg"),
                SharedString::from("settings-nav-themes.svg"),
                SharedString::from("settings-nav-wallpaper.svg"),
                SharedString::from("settings-nav-keyboard.svg"),
                SharedString::from("settings-nav-voice.svg"),
                SharedString::from("settings-nav-agents.svg"),
                SharedString::from("settings-nav-hooks.svg"),
                SharedString::from("settings-check-updates.svg"),
            ]),
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

    /// 内置壁纸资产：默认壁纸只由 bundle Resources 分发，不应重复嵌入主二进制。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   asset path = "wallpapers/1331865.webp"
    ///   WebP RIFF header = 12 bytes（RIFF + file size + WEBP）
    ///   packaged copy = Contents/Resources/wallpapers/1331865.webp
    ///   embedded copy = 0 bytes（避免与 packaged copy 重复）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. EmbeddedAssets.load(DEFAULT_WALLPAPER_ASSET) → 返回 None
    ///   2. EmbeddedAssets.load("wallpapers/missing.png") → 返回 None
    ///
    /// 预期结果：
    ///   - 正断言：默认壁纸不由 EmbeddedAssets 返回
    ///   - 负断言：未知壁纸同样不应返回任何嵌入字节
    #[test]
    fn test_embedded_assets_do_not_duplicate_packaged_wallpaper_bytes() {
        let assets = EmbeddedAssets;
        let default = assets
            .load(DEFAULT_WALLPAPER_ASSET)
            .expect("default wallpaper lookup should not fail");
        let missing = assets
            .load("wallpapers/missing.png")
            .expect("unknown wallpaper lookup should not fail");

        assert!(
            default.is_none(),
            "packaged default wallpaper must not also be embedded in the main binary"
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

    /// 内置资产目录：只枚举真正写入主二进制的图标，壁纸由 bundle Resources 分发。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   root = "" → 1 个顶层目录（icons）
    ///   wallpapers embedded entries = 0（11 张图片均只保留 bundle copy）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. list root/icons/wallpapers
    ///   2. 校验 root 暴露 icons，icons 非空，wallpapers 为空
    ///
    /// 预期结果：
    ///   - 正断言：root 精确包含 icons，且图标目录仍可枚举
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
        let wallpapers = assets
            .list("wallpapers")
            .expect("wallpaper embedded asset listing should not fail");

        assert_eq!(
            root,
            vec![SharedString::from("icons")],
            "root asset listing should expose only the actually embedded icons directory"
        );
        assert!(
            !icons.is_empty(),
            "embedded icons directory must remain available after wallpaper deduplication"
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
    ///   toolbar 路径数 = 12；首非空白字节期望 = b'<'（ASCII 60）；
    ///   负样本路径 = "icons/toolbar-missing.svg"。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 逐一 load 12 条 toolbar 路径并检查首非空白字节
    ///   2. load 缺失路径并期望 None
    ///
    /// 预期结果：
    ///   - 正断言：12 条路径均返回 Some 且首非空白字节为 b'<'
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
