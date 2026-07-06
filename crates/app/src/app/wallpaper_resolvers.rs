//! 内置壁纸的分类与资源路径解析（纯逻辑，无 GPUI 依赖）。
//!
//! 本模块只负责：
//! - 由文件扩展名判定 [`WallpaperKind`]（静态 / GIF / 视频）；
//! - 提供内置壁纸清单 [`builtin_wallpapers()`]；
//! - 将内置壁纸的相对资源名解析为运行时可读的绝对路径 [`resolve_builtin_asset()`]。
//!
//! 依赖项仅 `std`，便于单元测试在不打开窗口的前提下运行。

use std::path::{Path, PathBuf};

/// macOS app bundle 的 `Contents/Resources` 目录；非打包运行返回 None。
///
/// 打包态：`Foo.app/Contents/MacOS/<bin>` → `Foo.app/Contents/Resources`。
/// 开发态：`target/(debug|release)/<bin>`，parent 链不存在 `Contents/MacOS`，
/// 故返回 None —— 此时内置资源解析回退到 `CARGO_MANIFEST_DIR/assets`。
///
/// 严格校验三层布局（`MacOS` → `Contents` → `*.app`），避免误判任意目录。
pub(crate) fn bundle_resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // .../Foo.app/Contents/MacOS/<bin>
    let macos = exe.parent()?; // .../Contents/MacOS
    let contents = macos.parent()?; // .../Contents
    if macos.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let app_bundle = contents.parent()?;
    let app_name = app_bundle.file_name()?.to_str()?;
    if !app_name.ends_with(".app") {
        return None;
    }
    let res = contents.join("Resources");
    if res.is_dir() {
        Some(res)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallpaperKind {
    /// 静态图：.png / .jpg / .jpeg
    Static,
    /// 动态图：.gif
    Gif,
    /// 视频：.mp4 / .mov
    Video,
}

impl WallpaperKind {
    /// 由文件扩展名判定壁纸类别；未知扩展返回 None。
    ///
    /// 扩展名大小写不敏感（先 `to_ascii_lowercase` 归一化）。
    /// 调用方传入的 ext **不含** 前导点（如 `"png"` 而非 `".png"`）。
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" => Some(Self::Static),
            "gif" => Some(Self::Gif),
            "mp4" | "mov" => Some(Self::Video),
            _ => None,
        }
    }
}

/// 内置壁纸清单：固定 10 项，每项 `(相对资源路径, 显示名, kind)`。
///
/// 相对路径形如 `wallpapers/1331865.png`，是相对 app bundle 的 Resources 目录
/// （打包态）或 `crates/app/assets/` 目录（开发态）的子路径。
/// 素材布局：
///   - 3 张静态图（含默认 1331865.png）
///   - 2 张 GIF
///   - 5 段 MP4
pub(crate) fn builtin_wallpapers() -> &'static [(&'static str, &'static str, WallpaperKind)] {
    &[
        ("wallpapers/1331865.png", "Default", WallpaperKind::Static),
        ("wallpapers/1350954.png", "Aurora", WallpaperKind::Static),
        ("wallpapers/1358524.png", "Nebula", WallpaperKind::Static),
        ("wallpapers/215824.gif", "Drift", WallpaperKind::Gif),
        ("wallpapers/220240.gif", "Pulse", WallpaperKind::Gif),
        ("wallpapers/1403512.mp4", "Tide", WallpaperKind::Video),
        ("wallpapers/1404942.mp4", "Ember", WallpaperKind::Video),
        ("wallpapers/1406443.mp4", "Monsoon", WallpaperKind::Video),
        ("wallpapers/1406489.mp4", "Abyss", WallpaperKind::Video),
        ("wallpapers/1407174.mp4", "Halcyon", WallpaperKind::Video),
    ]
}

/// 把内置相对资源名（如 `wallpapers/1350954.png`）解析为运行时可读的绝对路径。
///
/// 解析顺序：
/// 1. **开发态**：`CARGO_MANIFEST_DIR/assets/<rel>` —— `cargo run` / `cargo test`
///    时 ManifestDir 即 `crates/app`，对应 `crates/app/assets/wallpapers/<rel>`。
/// 2. **打包态**：`bundle_resources_dir()/<rel>` —— `Foo.app/Contents/Resources`。
///
/// 任一来源命中且 `is_file()` 为真即返回；均不命中返回 None。
/// 调用方应对 None 回退到默认壁纸（`DEFAULT_WALLPAPER_ASSET`）。
pub(crate) fn resolve_builtin_asset(rel: &str) -> Option<PathBuf> {
    // 1) 开发态优先：CARGO_MANIFEST_DIR 在编译期注入，dev 构建一定有。
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let dev = Path::new(manifest).join("assets").join(rel);
        if dev.is_file() {
            return Some(dev);
        }
    }
    // 2) 打包态：app bundle 的 Contents/Resources/<rel>。
    if let Some(res) = bundle_resources_dir() {
        let packed = res.join(rel);
        if packed.is_file() {
            return Some(packed);
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_resolvers_tests.rs"]
mod tests;
