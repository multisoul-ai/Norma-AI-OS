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
    /// 静态图：.png / .jpg / .jpeg / .webp
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
            "png" | "jpg" | "jpeg" | "webp" => Some(Self::Static),
            "gif" => Some(Self::Gif),
            "mp4" | "mov" => Some(Self::Video),
            _ => None,
        }
    }

    /// 用户可见的运动状态标签。GIF 与视频都归入 Dynamic，避免用户必须先理解文件格式。
    pub(crate) const fn badge_label(self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::Gif | Self::Video => "Dynamic",
        }
    }
}

/// 内置壁纸清单：每项 `(相对资源路径, 显示名, kind)`。
///
/// 相对路径形如 `wallpapers/1331865.webp`，是相对 app bundle 的 Resources 目录
/// （打包态）或 `crates/app/assets/` 目录（开发态）的子路径。
/// 素材布局：
///   - 9 张静态图（含默认 1331865.webp 与 `mut/v2` 的 6 张）
///   - 2 张 GIF
///   - 5 段 MP4
pub(crate) fn builtin_wallpapers() -> &'static [(&'static str, &'static str, WallpaperKind)] {
    &[
        (
            crate::assets::HOME_BACKGROUND_ASSET,
            "Earth Horizon",
            WallpaperKind::Static,
        ),
        ("wallpapers/1331865.webp", "Classic", WallpaperKind::Static),
        ("wallpapers/1350954.webp", "Aurora", WallpaperKind::Static),
        ("wallpapers/1358524.webp", "Nebula", WallpaperKind::Static),
        (
            "wallpapers/74174.webp",
            "MUT v2 · 74174",
            WallpaperKind::Static,
        ),
        (
            "wallpapers/74176.jpg",
            "MUT v2 · 74176",
            WallpaperKind::Static,
        ),
        (
            "wallpapers/718916.jpg",
            "MUT v2 · 718916",
            WallpaperKind::Static,
        ),
        (
            "wallpapers/1411492.webp",
            "MUT v2 · 1411492",
            WallpaperKind::Static,
        ),
        (
            "wallpapers/379352.jpg",
            "MUT v2 · 379352",
            WallpaperKind::Static,
        ),
        (
            "wallpapers/1411385.webp",
            "MUT v2 · 1411385",
            WallpaperKind::Static,
        ),
        ("wallpapers/215824.gif", "Drift", WallpaperKind::Gif),
        ("wallpapers/220240.gif", "Pulse", WallpaperKind::Gif),
        ("wallpapers/1403512.mp4", "Tide", WallpaperKind::Video),
        ("wallpapers/1404942.mp4", "Ember", WallpaperKind::Video),
        ("wallpapers/1406443.mp4", "Monsoon", WallpaperKind::Video),
        ("wallpapers/1406489.mp4", "Abyss", WallpaperKind::Video),
        ("wallpapers/1407174.mp4", "Halcyon", WallpaperKind::Video),
    ]
}

/// 把持久化的内置壁纸路径归一化为当前清单中的 canonical 路径。
///
/// 旧版安装包中的 6 张 PNG 已转换为同名 WebP 以缩小包体。这里保留显式迁移表，
/// 让升级前保存的选择继续指向同一张壁纸；已经是当前路径时原样返回，未知路径拒绝。
pub(crate) fn canonical_builtin_wallpaper(rel: &str) -> Option<&'static str> {
    let canonical = match rel {
        "wallpapers/1331865.png" => "wallpapers/1331865.webp",
        "wallpapers/1350954.png" => "wallpapers/1350954.webp",
        "wallpapers/1358524.png" => "wallpapers/1358524.webp",
        "wallpapers/74174.png" => "wallpapers/74174.webp",
        "wallpapers/1411492.png" => "wallpapers/1411492.webp",
        "wallpapers/1411385.png" => "wallpapers/1411385.webp",
        current => current,
    };
    builtin_wallpapers()
        .iter()
        .find(|(registered, _, _)| *registered == canonical)
        .map(|(registered, _, _)| *registered)
}

/// 把内置相对资源名（如 `wallpapers/1350954.webp`）解析为运行时可读的绝对路径。
///
/// 解析顺序：
/// 1. **App bundle**：`bundle_resources_dir()/<rel>` —— `Foo.app/Contents/Resources`。
///    即使开发时由 `cargo run` 启动了临时 `.app`，也必须先用这一份，避免应用
///    读取位于用户“文稿”目录的源码 checkout 并触发 TCC 授权。
/// 2. **未打包开发态**：`CARGO_MANIFEST_DIR/assets/<rel>` —— `cargo test` 或直接运行
///    目标二进制时 ManifestDir 即 `crates/app`，对应 `crates/app/assets/wallpapers/<rel>`。
///
/// 打包态缺少资源时不读取源码 checkout；可选媒体由下载缓存补足。
/// 调用方应对 None 回退到默认壁纸（`DEFAULT_WALLPAPER_ASSET`）。
pub(crate) fn resolve_builtin_asset(rel: &str) -> Option<PathBuf> {
    let dev_assets =
        option_env!("CARGO_MANIFEST_DIR").map(|manifest| Path::new(manifest).join("assets"));
    resolve_builtin_asset_from_paths(
        rel,
        bundle_resources_dir().as_deref(),
        dev_assets.as_deref(),
    )
    .or_else(|| crate::media::cached_asset(rel))
}

/// Small, signed stills keep the full library visible without downloading clips.
pub(crate) fn builtin_preview_asset(rel: &str) -> Option<PathBuf> {
    let registered = canonical_builtin_wallpaper(rel)?;
    let stem = Path::new(registered).file_stem()?.to_str()?;
    resolve_builtin_asset(&format!("wallpaper-previews/{stem}.webp"))
}

/// 根据已知的 bundle 与未打包开发资源根目录解析一个内置壁纸。
///
/// 这个边界接受显式根目录，便于在不启动 macOS App 的情况下验证优先级：bundle
/// 资源始终高于源码资源；仅当不存在 bundle 时，才允许读取开发源码资源。
pub(crate) fn resolve_builtin_asset_from_paths(
    rel: &str,
    bundle_resources: Option<&Path>,
    dev_assets: Option<&Path>,
) -> Option<PathBuf> {
    let candidate = bundle_resources.or(dev_assets)?.join(rel);
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_resolvers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_packaging_assets_tests.rs"]
mod packaging_assets_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_on_demand_tests.rs"]
mod on_demand_tests;
