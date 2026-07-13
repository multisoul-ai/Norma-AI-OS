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
