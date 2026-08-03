//! E2E-11：Launchpad 固定视频背景。

use app::AppScreen;
use gpui::{px, size, TestAppContext};

use crate::harness::{echo_factory, make_app};

/// E2E-11：新窗口启动时 Launchpad 必须绑定指定的 home 视频，而不是上一版视频或 Canvas 默认图片。
///
/// 数据构造（含关键数值的推导过程）：
///   headless window = 1 个（TestAppContext::single）
///   Launchpad 视频 = "wallpapers/home.mp4"（1280×720 固定应用资源）
///   上一版 Launchpad 视频 = "wallpapers/1406443.mp4"（3840×2160）
///   Canvas 默认图 = "wallpapers/1331865.webp"（不同扩展名与资源路径）
///   三个资源各 1 条，故生产资源必须与两个旧/画布资源都不相等。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. make_app 装配真实 MultisoulApp 渲染树，初始 screen 为 Launchpad
///   2. 通过稳定 e2e 访问器读取 Launchpad 背景资源路径
///   3. 比较固定视频与 Canvas 默认图片资源，确认没有复用画布背景选择
///
/// 预期结果：
///   - 正断言：初始页面是 Launchpad
///   - 正断言：Launchpad 背景资源精确为 wallpapers/home.mp4
///   - 负断言：Launchpad 背景资源不再是 wallpapers/1406443.mp4
///   - 负断言：Launchpad 背景资源不应为 Canvas 默认的 1331865.webp
#[test]
fn e2e_11_launchpad_boots_with_dedicated_home_video() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let (screen, video_asset) = app.update(cx, |app, _cx| {
        (app.screen(), app.launchpad_video_asset_e2e())
    });

    assert_eq!(
        screen,
        AppScreen::Launchpad,
        "应用初始渲染应处于 Launchpad，才能展示启动页的固定视频"
    );
    assert_eq!(
        video_asset, "wallpapers/home.mp4",
        "Launchpad 默认背景必须播放用户指定的 home 视频"
    );
    assert_ne!(
        video_asset, "wallpapers/1406443.mp4",
        "Launchpad 默认背景不得继续残留上一版 1406443.mp4"
    );
    assert_ne!(
        video_asset, "wallpapers/1331865.webp",
        "Launchpad 不得继承 Canvas 的默认 WebP 背景"
    );
}

/// E2E-11b：宽屏 Launchpad 应完全移除右侧 Workspace preview 区域。
///
/// 数据构造（含关键数值的推导过程）：
///   headless window = 1440×900px，旧实现会在该 Wide 模式尺寸挂载右侧预览
///   旧右侧区域组件数 = 1 个容器 + 1 个内容层
///   目标右侧区域组件数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. make_app 装配真实 Launchpad 渲染树，并将窗口调整为 1440×900。
///   2. 确认应用仍处于 Launchpad，避免因页面未渲染造成假阴性。
///   3. 查询旧预览容器与内容层的稳定 debug selector。
///
/// 预期结果：
///   - 正断言：宽屏调整后仍渲染 Launchpad 主页面。
///   - 负断言：Workspace preview 根容器不得出现在渲染树中。
///   - 负断言：Workspace preview 内容层不得作为孤儿残留。
#[test]
fn e2e_11_launchpad_removes_entire_workspace_preview_region() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();

    let screen = app.update(cx, |app, _cx| app.screen());

    assert_eq!(
        screen,
        AppScreen::Launchpad,
        "宽屏调整后应用仍应处于 Launchpad，确保预览缺失不是页面未渲染造成的假阴性"
    );
    assert!(
        cx.debug_bounds("launchpad-workspace-preview").is_none(),
        "用户框选的整个 Workspace preview 根容器应从宽屏 Launchpad 删除"
    );
    assert!(
        cx.debug_bounds("launchpad-workspace-preview-content")
            .is_none(),
        "Workspace preview 内容层不得脱离根容器后作为孤儿残留"
    );
}
