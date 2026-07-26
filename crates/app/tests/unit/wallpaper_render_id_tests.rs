//! T-W10: render_id_key 契约测试 —— 防止 GPUI 0.2.2 跨源复用 ImgState 的越界 panic。
//!
//! 独立文件以守住 `wallpaper_settings_tests.rs` 的 500 行上限。挂在
//! `settings/wallpaper.rs` 的 `mod render_id_tests`（与 `mod tests` 同级）。

use super::state::{WallpaperSelection, WallpaperSettingsState};
use super::tests::TestRootGuard;

/// T-W10: render_id_key 必须随当前选择变化，否则 GPUI 0.2.2 会跨源复用 ImgState。
///
/// 背景（回归根因）：GPUI 0.2.2 把 `ImgState.frame_index` 按 element-id 跨渲染保留。
/// 多帧 GIF 能把 frame_index 推进到 >1；若随后切到单帧静态图且壁纸 img 仍用同一 id，
/// 旧 frame_index 会索引新单帧 RenderImage.data → `index out of bounds` panic
/// （gpui assets.rs:79）。修复让 img().id 由 render_id_key 派生，切换源即换 id。
/// 本测试验证 render_id_key 的核心契约：不同选择 → 不同 key。
///
/// 数据构造（含关键数值的推导过程）：
///   s = load_from_store(temp/app.sqlite) → 默认选择 Builtin{1331865.png}
///   switch1 = select_builtin("wallpapers/220240.gif") → Builtin{220240.gif}（多帧 GIF）
///   switch2 = select_custom("mine.png") → Custom{mine.png}（单帧静态图）
///   各步对应 key：
///     默认    = "builtin:wallpapers/1331865.png"
///     switch1 = "builtin:wallpapers/220240.gif"
///     switch2 = "custom:mine.png"
///   三者必须互不相同。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 载入默认 state → 读 render_id_key（k_default）
///   2. select_builtin(gif) → 读 render_id_key（k_gif）
///   3. select_custom(png) → 读 render_id_key（k_custom）
///   4. 比较三个 key
///
/// 预期结果：
///   - 正断言：k_default 与 k_gif 不同（切换内置源 → key 变）
///   - 正断言：k_gif 与 k_custom 不同（内置→自定义 → key 变）
///   - 正断言：k_default 与 k_custom 不同
///   - 负断言：三者不应相等，否则 GPUI 会跨源复用 ImgState 触发越界 panic
#[test]
fn test_render_id_key_changes_with_selection() {
    let _g = TestRootGuard::new();
    let mut s = WallpaperSettingsState::load_from_store(_g.settings_store());

    let k_default = s.render_id_key();
    s.select_builtin("wallpapers/220240.gif");
    let k_gif = s.render_id_key();
    s.select_custom("mine.png".to_string());
    let k_custom = s.render_id_key();

    assert_ne!(
        k_default, k_gif,
        "render_id_key 必须随内置源切换而变；相同会导致 GPUI 跨源复用 ImgState 越界 panic"
    );
    assert_ne!(
        k_gif, k_custom,
        "render_id_key 必须在内置→自定义切换时变化；相同会越界 panic"
    );
    assert_ne!(
        k_default, k_custom,
        "render_id_key 在默认与自定义间必须不同；相同会越界 panic"
    );
    assert!(
        k_default.starts_with("builtin:")
            && k_gif.starts_with("builtin:")
            && k_custom.starts_with("custom:"),
        "render_id_key 应带 builtin:/custom: 前缀区分来源，实际 default={k_default} gif={k_gif} custom={k_custom}"
    );
}

// 显式确认 WallpaperSelection 仍可按需构造，避免误删 enum 导致测试静默通过。
#[test]
fn test_wallpaper_selection_variants_are_distinct() {
    let b = WallpaperSelection::Builtin {
        rel: "wallpapers/x.png",
    };
    let c = WallpaperSelection::Custom {
        filename: "y.png".to_string(),
    };
    assert!(
        matches!(&b, WallpaperSelection::Builtin { rel } if *rel == "wallpapers/x.png"),
        "Builtin 变体应携带 rel 路径"
    );
    assert!(
        matches!(&c, WallpaperSelection::Custom { filename } if filename == "y.png"),
        "Custom 变体应携带 filename"
    );
}
