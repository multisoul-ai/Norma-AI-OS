/// 工作台背景装饰线：保留壁纸与深色遮罩，但不再渲染圆环或底部细线。
///
/// 数据构造（含关键数值的推导过程）：
///   左侧圆环 = 520px × 520px，圆角半径 = 520 / 2 = 260px，左偏移 = -160px
///   右侧圆环 = 620px × 620px，圆角半径 = 620 / 2 = 310px，右偏移 = -180px
///   底部基线 = 1px 高，距底部 = 84px，左右各缩进 = 90px
///   保留层 = 当前 wallpaper layer + graphite 深色渐变 shade
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 `render_workspace_backdrop` 的生产渲染源码。
///   2. 确认背景仍挂载当前壁纸和深色渐变遮罩。
///   3. 逐一检查两个圆环的固定定位参数和底部 1px 基线参数均已移除。
///
/// 预期结果：
///   - 正断言：壁纸层和深色渐变遮罩继续存在，Canvas 背景不会变成空白。
///   - 负断言：左侧青色圆环不应存在。
///   - 负断言：右侧橙色圆环不应存在。
///   - 负断言：底部青色 1px 基线不应存在。
#[test]
fn test_workspace_backdrop_keeps_wallpaper_and_shade_without_decorative_lines() {
    let source = include_str!("../../src/app/chrome.rs");
    let backdrop_source = source
        .split("pub(crate) fn render_wallpaper_layer")
        .next()
        .unwrap_or(source);

    assert!(
        backdrop_source.contains("render_wallpaper_layer()"),
        "移除装饰线后工作台背景仍应挂载当前壁纸层"
    );
    assert!(
        backdrop_source.contains("WALLPAPER_SHADE_TOP_ALPHA")
            && backdrop_source.contains("WALLPAPER_SHADE_BOTTOM_ALPHA"),
        "移除装饰线后工作台背景仍应保留 graphite 深色渐变遮罩"
    );
    assert!(
        !backdrop_source.contains("left(px(-160.0))")
            && !backdrop_source.contains("w(px(520.0))")
            && !backdrop_source.contains("rounded(px(260.0))"),
        "工作台背景不应继续渲染截图左上与左下可见的 520px 青色圆环"
    );
    assert!(
        !backdrop_source.contains("right(px(-180.0))")
            && !backdrop_source.contains("w(px(620.0))")
            && !backdrop_source.contains("rounded(px(310.0))"),
        "工作台背景不应继续渲染截图右侧可见的 620px 橙色圆环"
    );
    assert!(
        !backdrop_source.contains("bottom(px(84.0))") && !backdrop_source.contains("h(px(1.0))"),
        "工作台背景不应继续渲染距底部 84px 的 1px 青色基线"
    );
}
