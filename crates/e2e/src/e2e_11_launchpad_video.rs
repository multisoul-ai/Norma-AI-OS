//! E2E-11：Launchpad 固定视频背景。

use app::AppScreen;
use gpui::TestAppContext;

use crate::harness::{echo_factory, make_app};

/// E2E-11：新窗口启动时 Launchpad 必须绑定 home.mp4，而不是 Canvas 默认图片。
///
/// 数据构造（含关键数值的推导过程）：
///   headless window = 1 个（TestAppContext::single）
///   Launchpad 视频 = "wallpapers/home.mp4"（固定应用资源）
///   Canvas 默认图 = "wallpapers/1331865.png"（不同扩展名与资源路径）
///   两个资源各 1 条，故字符串必须不相等。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. make_app 装配真实 MultisoulApp 渲染树，初始 screen 为 Launchpad
///   2. 通过稳定 e2e 访问器读取 Launchpad 背景资源路径
///   3. 比较固定视频与 Canvas 默认图片资源，确认没有复用画布背景选择
///
/// 预期结果：
///   - 正断言：初始页面是 Launchpad
///   - 正断言：Launchpad 背景资源精确为 wallpapers/home.mp4
///   - 负断言：Launchpad 背景资源不应为 Canvas 默认的 1331865.png
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
        "Launchpad 默认背景必须播放用户指定的 home.mp4"
    );
    assert_ne!(
        video_asset, "wallpapers/1331865.png",
        "Launchpad 不得继承 Canvas 的默认 PNG 背景"
    );
}
