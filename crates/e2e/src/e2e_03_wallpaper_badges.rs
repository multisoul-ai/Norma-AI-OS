//! Settings → Wallpaper 媒体类型标签的真实渲染树验收。

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Wallpaper 卡片：静态图、GIF 与视频都显示紧凑标签，并正确归入静态/动态两组。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in wallpapers = static(9) + gif(2) + video(5) = 16 张
///   representative cards = Default(index 0, Static)
///                        + Drift(index 9, Dynamic GIF)
///                        + Tide(index 11, Dynamic Video) = 3 张
///   badge height = 20px；compact width upper bound = 80px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas，并打开 Settings → Wallpaper。
///   2. 从真实 GPUI 渲染树读取 Default 的 Static 标签 bounds。
///   3. 分别读取 Drift GIF 与 Tide Video 的 Dynamic 标签 bounds。
///   4. 检查三个标签高度与宽度，确认其是紧凑胶囊而非遮挡缩略图的大色块。
///
/// 预期结果：
///   - 正断言：Static、GIF Dynamic、Video Dynamic 三个代表标签均真实存在。
///   - 正断言：三个标签均为 20px 高，且宽度不超过 80px。
///   - 负断言：Default 不得出现 Dynamic 标签；Drift 与 Tide 不得出现 Static 标签。
#[test]
fn e2e_03_wallpaper_cards_show_compact_static_and_dynamic_badges() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("应创建隔离的 Wallpaper 工作区");
    enter_canvas(&app, cx, temporary_root.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-wallpaper");

    let static_badge = required_bounds(cx, "settings-wallpaper-badge-0-static");
    let gif_badge = required_bounds(cx, "settings-wallpaper-badge-9-dynamic");
    let video_badge = required_bounds(cx, "settings-wallpaper-badge-11-dynamic");

    assert_compact_badge(static_badge, "Default Static");
    assert_compact_badge(gif_badge, "Drift GIF Dynamic");
    assert_compact_badge(video_badge, "Tide Video Dynamic");

    assert!(
        cx.debug_bounds("settings-wallpaper-badge-0-dynamic")
            .is_none(),
        "Default 静态图不得同时渲染 Dynamic 标签"
    );
    assert!(
        cx.debug_bounds("settings-wallpaper-badge-9-static")
            .is_none(),
        "Drift GIF 不得渲染 Static 标签"
    );
    assert!(
        cx.debug_bounds("settings-wallpaper-badge-11-static")
            .is_none(),
        "Tide Video 不得渲染 Static 标签"
    );
}

fn assert_compact_badge(bounds: Bounds<Pixels>, description: &str) {
    assert_eq!(
        f32::from(bounds.size.height),
        20.0,
        "{description} 标签高度必须为 20px，保持 macOS 式紧凑胶囊比例"
    );
    assert!(
        f32::from(bounds.size.width) <= 80.0,
        "{description} 标签宽度不得超过 80px，避免遮挡壁纸缩略图"
    );
    assert!(
        f32::from(bounds.size.width) >= 44.0,
        "{description} 标签宽度不得小于 44px，需完整容纳可读文案"
    );
}

fn required_bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.run_until_parked();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应存在于 Wallpaper 真实渲染树"))
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = required_bounds(cx, selector);
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}
