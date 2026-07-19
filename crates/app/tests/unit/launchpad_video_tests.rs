//! Launchpad 专用视频背景测试。与通用布局测试拆分，保证每个测试文件不超过 500 行。

use super::*;

/// L-0: Launchpad 固定播放指定的 home 视频，且不继承 Canvas 的壁纸选择。
///
/// 数据构造（含关键数值的推导过程）：
///   Launchpad 指定视频资源 = "wallpapers/home.mp4"（1280×720）
///   上一版 Launchpad 视频 = "wallpapers/1406443.mp4"（3840×2160）
///   Canvas 可选内置视频 = "wallpapers/1403512.mp4"（设置页清单中的另一条视频）
///   三个资源路径各 1 条；默认路径必须命中 home，且不能残留上一版资源。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 launchpad_default_video_path() 解析 Launchpad 专用资源
///   2. 将路径与上一版 1406443.mp4、Canvas 另一条视频进行比较
///   3. 检查解析结果实际指向打包资源目录的 home.mp4
///
/// 预期结果：
///   - 正断言：Launchpad 默认视频文件名为 home.mp4 且文件存在
///   - 正断言：Launchpad 资源路径包含 wallpapers/home.mp4
///   - 负断言：Launchpad 默认路径不应继续使用上一版 1406443.mp4
///   - 负断言：Launchpad 默认路径不应等于 Canvas 可选视频 1403512.mp4
#[test]
fn test_launchpad_uses_home_video_independent_of_canvas_wallpaper() {
    let launchpad_video =
        launchpad_default_video_path().expect("Launchpad 指定的 home.mp4 应能从应用资源目录解析");
    let previous_video =
        crate::app::wallpaper_resolvers::resolve_builtin_asset("wallpapers/1406443.mp4")
            .expect("上一版 Launchpad 1406443.mp4 应可用于负向回归比较");
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
        launchpad_video, previous_video,
        "Launchpad 默认视频不得继续残留上一版 1406443.mp4"
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
        "Launchpad 模块必须声明用户指定的 home 视频资源路径"
    );
    assert!(
        source_mod.contains("render_launchpad_video_layer"),
        "Launchpad render_launchpad 必须挂载专用视频层以播放 home 默认资源"
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

/// L-10：Launchpad 生产默认必须使用实验胜出的 balanced 轻遮罩。
///
/// 数据构造（含关键数值的推导过程）：
///   balanced 水平 alpha = 0x28 / 0x18，垂直 alpha = 0x18 / 0x30
///   balanced 左侧合成不透明度 = 1 - (1 - 40/255) × (1 - 24/255) ≈ 23.6%
///   balanced 右侧合成不透明度 = 1 - (1 - 24/255) × (1 - 48/255) ≈ 26.5%
///   旧左侧水平 alpha = 0xf4；它会与垂直 0x94 合成约 98.2% 的高遮罩
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取渲染路径直接使用的 `LaunchpadBackdropOverlay::BALANCED`
///   2. 逐一检查两层渐变的四个端点
///   3. 与旧高遮罩和全透明值做负向比较
///
/// 预期结果：
///   - 正断言：四个端点精确为 0x28/0x18/0x18/0x30
///   - 负断言：左侧不得继续使用旧 0xf4 高遮罩
///   - 负断言：四个端点不得全部为零，保证文字仍有基础对比层
#[test]
fn test_launchpad_uses_balanced_overlay_as_production_default() {
    let overlay = LaunchpadBackdropOverlay::BALANCED;

    assert_eq!(
        overlay.horizontal_start, 0x28,
        "生产默认水平渐变左端必须使用实验胜出的 balanced alpha"
    );
    assert_eq!(
        overlay.horizontal_end, 0x18,
        "生产默认水平渐变右端必须保持轻遮罩，不能洗掉视频细节"
    );
    assert_eq!(
        overlay.vertical_start, 0x18,
        "生产默认垂直渐变上端必须使用 balanced alpha"
    );
    assert_eq!(
        overlay.vertical_end, 0x30,
        "生产默认垂直渐变下端必须保留约 19% 单层遮罩以支撑可读性"
    );
    assert_ne!(
        overlay.horizontal_start, 0xf4,
        "生产默认不得继续使用会把背景合成遮到约 98.2% 的旧左端 alpha"
    );
    assert!(
        overlay.horizontal_start != 0
            || overlay.horizontal_end != 0
            || overlay.vertical_start != 0
            || overlay.vertical_end != 0,
        "生产默认不得变成全透明遮罩，否则左侧正文会失去基础对比层"
    );
}

/// L-11：生产 Launchpad 视频路径必须固定，不得保留实验环境变量覆盖入口。
///
/// 数据构造（含关键数值的推导过程）：
///   生产资源 = wallpapers/home.mp4（1280×720）
///   禁止环境变量 = NORMA_HD_BENCHMARK_VIDEO / NORMA_HD_BENCHMARK_OVERLAY
///   禁止实验解析函数 = launchpad_video_path_from_override
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过生产 resolver 解析固定默认路径
///   2. 读取 Launchpad 组装源码，检查实验变量与覆盖函数是否仍存在
///   3. 确认解析结果仍是可打开的 home.mp4 文件
///
/// 预期结果：
///   - 正断言：生产 resolver 返回 home.mp4 且文件存在
///   - 负断言：源码不得包含两个 NORMA_HD_BENCHMARK 环境变量
///   - 负断言：源码不得保留实验路径覆盖函数
#[test]
fn test_launchpad_production_path_has_no_benchmark_override() {
    let source_mod = include_str!("../../src/workspace/launchpad/mod.rs");
    let production =
        launchpad_default_video_path().expect("生产 Launchpad 应始终能解析固定的 home 内置视频");

    assert_eq!(
        production.file_name().and_then(|name| name.to_str()),
        Some("home.mp4"),
        "生产 Launchpad resolver 必须稳定返回用户指定的 home 视频"
    );
    assert!(
        production.is_file(),
        "生产 Launchpad resolver 返回的 home 路径必须是可打开的真实文件"
    );
    assert!(
        !source_mod.contains("NORMA_HD_BENCHMARK_VIDEO"),
        "生产 Launchpad 源码不得允许环境变量把固定 home 默认资源改回实验候选"
    );
    assert!(
        !source_mod.contains("NORMA_HD_BENCHMARK_OVERLAY"),
        "生产 Launchpad 源码不得允许环境变量重新启用 current/clear 实验遮罩"
    );
    assert!(
        !source_mod.contains("launchpad_video_path_from_override"),
        "生产 Launchpad 源码不得保留绕过固定资源 resolver 的实验函数"
    );
}
