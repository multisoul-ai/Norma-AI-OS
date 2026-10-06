use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, TestAppContext};

/// 首页只在标题附近提供阅读衬底，壁纸不会被整页浅色遮罩冲淡。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个空工作空间；1280×800 窗口面积 = 1,024,000px²。
///   标题衬底面积上限 = 窗口面积 / 20 = 51,200px²，即最多覆盖 5%。
///   Paper Blue 与 Sapphire Night 分别覆盖浅、深主题，不创建项目或 PTY。
/// 执行过程：
///   1. 打开隔离首页，分别绘制浅、深主题。
///   2. 读取壁纸、标题阅读衬底和首张卡片的真实布局边界。
///
/// 预期结果：
///   - 正断言：壁纸铺满窗口；标题有局部衬底且不超过窗口面积的 5%。
///   - 负断言：阅读衬底不能覆盖整窗、顶部栏或工作空间卡片。
#[test]
fn desktop_home_reading_surface_stays_local_to_heading() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化数据库");
    let catalog = store.desktop_workspace_store();
    catalog.create("Design").expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    for id in [ui::ThemeId::PaperBlue, ui::ThemeId::SapphireNight] {
        host.update(cx, |app, cx| {
            app.screen = AppScreen::Launchpad;
            app.theme_id = id;
            app.theme = ui::Theme::for_id(id);
            app.desktop_workspace.workspaces = catalog.list().expect("应加载目录");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let wallpaper = cx
            .debug_bounds("desktop-home-wallpaper")
            .expect("应显示壁纸");
        let reading = cx
            .debug_bounds("desktop-home-heading-surface")
            .expect("标题必须有局部阅读衬底");
        let toolbar = cx
            .debug_bounds("desktop-home-toolbar")
            .expect("应显示顶部栏");
        let card = cx
            .debug_bounds("desktop-home-card-0")
            .expect("应显示工作空间卡片");
        assert_eq!(
            wallpaper.size,
            size(px(1280.0), px(800.0)),
            "{id:?} 壁纸必须完整铺满窗口"
        );
        let reading_area = f32::from(reading.size.width) * f32::from(reading.size.height);
        // 1280 × 800 / 20 = 51,200：阅读保护限于标题附近，不能压低全屏饱和度。
        assert!(
            reading_area <= 51_200.0,
            "{id:?} 阅读衬底不能超过窗口面积的 5%"
        );
        assert_ne!(reading, wallpaper, "{id:?} 阅读衬底不能成为整页遮罩");
        assert!(
            reading.top() >= toolbar.bottom(),
            "{id:?} 阅读衬底不能覆盖顶部栏"
        );
        assert!(
            reading.bottom() < card.top(),
            "{id:?} 阅读衬底不能遮住工作空间卡片"
        );
    }
}
