//! Launchpad 专用视频背景测试。与通用布局测试拆分，保证每个测试文件不超过 500 行。

use super::*;

/// L-0: Launchpad 固定播放 home.mp4，且不继承 Canvas 的壁纸选择。
///
/// 数据构造（含关键数值的推导过程）：
///   Launchpad 内置视频资源 = "wallpapers/home.mp4"（用户指定的 home.mp4）
///   Canvas 可选内置视频 = "wallpapers/1403512.mp4"（设置页清单中的另一条视频）
///   两个资源路径各 1 条，必须不相等，避免 Launchpad 跟随 Canvas 的选择。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 launchpad_default_video_path() 解析 Launchpad 专用资源
///   2. 将路径与 Canvas 壁纸清单中的一条视频进行比较
///   3. 检查解析结果实际指向打包资源目录的 home.mp4
///
/// 预期结果：
///   - 正断言：Launchpad 默认视频文件名为 home.mp4 且文件存在
///   - 正断言：Launchpad 资源路径包含 wallpapers/home.mp4
///   - 负断言：Launchpad 默认路径不应等于 Canvas 可选视频 1403512.mp4
#[test]
fn test_launchpad_uses_dedicated_home_video_independent_of_canvas_wallpaper() {
    let launchpad_video =
        launchpad_default_video_path().expect("Launchpad 默认视频 home.mp4 应能从应用资源目录解析");
    let canvas_video =
        crate::app::wallpaper_resolvers::resolve_builtin_asset("wallpapers/1403512.mp4")
            .expect("Canvas 内置视频 1403512.mp4 应能从应用资源目录解析");

    assert_eq!(
        launchpad_video.file_name().and_then(|name| name.to_str()),
        Some("home.mp4"),
        "Launchpad 默认播放文件必须是用户指定的 home.mp4"
    );
    assert!(
        launchpad_video.ends_with("wallpapers/home.mp4"),
        "Launchpad 默认视频必须位于打包的 wallpapers/home.mp4 资源路径"
    );
    assert_ne!(
        launchpad_video, canvas_video,
        "Launchpad 默认视频不得复用 Canvas 当前可选壁纸视频"
    );
}

/// L-9: Launchpad 走专用的视频层，且不再复用 Canvas 壁纸渲染层。
///
/// 数据构造（含关键数值的推导过程）：
///   source_mod      = launchpad/mod.rs（render_launchpad 组装入口）
///   source_backdrop = launchpad/backdrop.rs（背景层构建）
///   专用视频契约    = launchpad_default_video_path + render_launchpad_video_layer
///   画布复用调用数  = 0（self.render_wallpaper_layer( 不得出现在 Launchpad 模块中）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 launchpad/mod.rs，确认使用 home.mp4 常量和专用路径解析函数
///   2. 读取 launchpad/mod.rs，确认调用 render_launchpad_video_layer
///   3. 读取 launchpad/backdrop.rs，确认不再内联 img() 渲染静态图片
///   4. 读取 launchpad/backdrop.rs，确认只接收专用 video Div
///
/// 预期结果：
///   - 正断言：mod.rs 包含 home.mp4 和专用视频层调用
///   - 负断言：mod.rs 不应调用 Canvas 的 render_wallpaper_layer
///   - 负断言：backdrop.rs 不应包含 img( 或 ImageSource
#[test]
fn test_launchpad_backdrop_uses_dedicated_video_layer() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");
    let source_backdrop = include_str!("../../src/workspace/launchpad/backdrop.rs");

    assert!(
        source_mod.contains("wallpapers/home.mp4"),
        "Launchpad 模块必须声明用户指定的 home.mp4 资源路径"
    );
    assert!(
        source_mod.contains("render_launchpad_video_layer"),
        "Launchpad render_launchpad 必须挂载专用视频层以播放 home.mp4"
    );
    assert!(
        !source_mod.contains("self.render_wallpaper_layer("),
        "Launchpad 不得复用 Canvas 的 render_wallpaper_layer，否则会继承画布壁纸选择"
    );
    assert!(
        !source_backdrop.contains("img("),
        "Launchpad backdrop 不应再内联 img() 渲染静态图片——这会让 Video 回退到 PNG，\
         且 gif 切换时缺少源派生 id 触发 frame_index 越界 panic"
    );
    assert!(
        !source_backdrop.contains("ImageSource"),
        "Launchpad backdrop 签名不应再接收 ImageSource，避免重新与 Canvas 的图片管线耦合"
    );
}

/// L-12: Launchpad 视频上方应只叠加一层 80% opacity 遮罩。
///
/// 数据构造（含关键数值的推导过程）：
///   opacity = 80% = 0.80 × 255 = 204 = 0xCC
///   视频可见比例 = 1 - 0.80 = 20%
///   遮罩层数量 = 1；若叠加两层 80%，综合 opacity = 1 - (1 - 0.8)² = 96%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 backdrop 模块定义的遮罩 alpha。
///   2. 读取真实 backdrop.rs 元素树，统计全屏 linear-gradient 遮罩数量。
///   3. 确认渐变两端都复用同一个 80% alpha 常量。
///
/// 预期结果：
///   - 正断言：遮罩 alpha 精确为 0xCC，即 80% opacity。
///   - 正断言：只保留一个渐变遮罩，背景视频仍有 20% 可见度。
///   - 负断言：不得保留两层遮罩形成 96% 的过度遮蔽。
#[test]
fn test_launchpad_backdrop_uses_single_eighty_percent_overlay() {
    let source_backdrop = include_str!("../../src/workspace/launchpad/backdrop.rs");
    let gradient_count = source_backdrop.matches(".bg(linear_gradient(").count();
    let alpha_usage_count = source_backdrop.matches("LAUNCHPAD_OVERLAY_ALPHA").count();

    assert_eq!(
        super::backdrop::LAUNCHPAD_OVERLAY_ALPHA,
        0xcc,
        "Launchpad 覆盖层应精确使用 0xCC，即 204/255 = 80% opacity"
    );
    assert_eq!(
        gradient_count, 1,
        "Launchpad 视频上方应只保留一个渐变遮罩，避免两层 80% 叠加为 96%"
    );
    assert_eq!(
        alpha_usage_count, 3,
        "80% alpha 常量应出现 3 次：定义 1 次、渐变两端各使用 1 次"
    );
    assert_ne!(gradient_count, 2, "Launchpad 不得继续保留两层全屏渐变遮罩");
}
