//! Embedded GPUI assets that should travel with the application binary.

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub const DEFAULT_WALLPAPER_ASSET: &str = "wallpapers/1331865.png";

const DEFAULT_WALLPAPER_BYTES: &[u8] = include_bytes!("../assets/wallpapers/1331865.png");

#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path {
            DEFAULT_WALLPAPER_ASSET => Ok(Some(Cow::Borrowed(DEFAULT_WALLPAPER_BYTES))),
            _ => Ok(None),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        match path {
            "" => Ok(vec![SharedString::from("wallpapers")]),
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

    /// 内置资产目录：GPUI 查询 wallpapers 目录时应能枚举默认壁纸文件名。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   root list path = "" → 1 个顶层目录 wallpapers
    ///   wallpapers list path = "wallpapers" → 1 个文件 1331865.png
    ///   unknown list path = "missing" → 0 个条目
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. list("") → 返回 wallpapers
    ///   2. list("wallpapers") → 返回 1331865.png
    ///   3. list("missing") → 返回空列表
    ///
    /// 预期结果：
    ///   - 正断言：root 包含 wallpapers
    ///   - 正断言：wallpapers 包含 1331865.png
    ///   - 负断言：missing 不包含任何条目
    #[test]
    fn test_embedded_assets_list_wallpaper_directory() {
        let assets = EmbeddedAssets;

        let root = assets
            .list("")
            .expect("root embedded asset listing should not fail");
        let wallpapers = assets
            .list("wallpapers")
            .expect("wallpaper embedded asset listing should not fail");
        let missing = assets
            .list("missing")
            .expect("unknown embedded asset listing should not fail");

        assert_eq!(
            root,
            vec![SharedString::from("wallpapers")],
            "root asset listing should expose the wallpapers directory"
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
}
