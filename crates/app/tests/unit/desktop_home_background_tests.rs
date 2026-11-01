use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, TestAppContext};

/// 首页全局壁纸铺满全窗，紧凑品牌区位于搜索和最近访问之前。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个空工作空间；1280×800 窗口面积 = 1,024,000px²。
///   品牌行高76px；左右20px安全距，问候之后留32px再进入搜索。
///   Paper Blue 与 Sapphire Night 分别覆盖浅、深主题，不创建项目或 PTY。
/// 执行过程：
///   1. 打开隔离首页，分别绘制浅、深主题。
///   2. 读取固定背景、标题区域和首张卡片的真实布局边界。
///
/// 预期结果：
///   - 正断言：背景铺满窗口；品牌行76px高，搜索与卡片依次排布。
///   - 负断言：旧标题胶囊和常驻底部菜单不再显示，标题区域不能覆盖顶部栏或卡片。
#[test]
fn desktop_home_global_wallpaper_and_brand_hierarchy() {
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
            .debug_bounds("desktop-home-brand")
            .expect("首页必须显示头像与问候");
        let toolbar = cx
            .debug_bounds("desktop-home-toolbar")
            .expect("应显示顶部栏");
        let card = cx
            .debug_bounds("desktop-home-card-0")
            .expect("应显示工作空间卡片");
        assert!(
            cx.debug_bounds("workspace-command-dock").is_none(),
            "{id:?} 首页不应常驻遮挡卡片的底部菜单"
        );
        assert_eq!(
            wallpaper.size,
            size(px(1280.0), px(800.0)),
            "{id:?} 壁纸必须完整铺满窗口"
        );
        assert_eq!(reading.size.height, px(76.0), "{id:?} 品牌使用紧凑76px行");
        let search = cx
            .debug_bounds("desktop-workspace-search")
            .expect("搜索胶囊");
        assert_eq!(
            search.top() - reading.bottom(),
            px(32.0),
            "品牌和搜索间距32px"
        );
        assert!(search.bottom() < card.top(), "搜索不能压住最近卡片");
        assert!(
            cx.debug_bounds("desktop-home-brand-surface").is_none(),
            "{id:?} 旧标题胶囊必须移除"
        );
        assert_ne!(reading, wallpaper, "{id:?} 标题不能成为整页遮罩");
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

/// 场景：返回首页时尊重全局壁纸，不能被旧首页固定地球图覆盖。
/// 数据：隔离宿主，全局选择Blue Horizon，与默认Earth不同。
/// 执行：绑定首页壁纸后读取有效选择；再把全局改回默认并重新绑定。
/// 预期：两次都严格等于用户全局选择，没有首页硬编码覆盖。
#[test]
fn desktop_home_wallpaper_binding_uses_global_selection() {
    use crate::settings::wallpaper::state::WallpaperSelection;
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化数据库");
    let mut test = TestAppContext::single();
    let host = test.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    host.update(&mut test, |app, cx| {
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.home_directory = true;
        let alternate = WallpaperSelection::Builtin {
            rel: "wallpapers/norma-blue-horizon.png",
        };
        app.wallpaper_settings.preferences.global = alternate.clone();
        app.bind_desktop_wallpaper(cx);
        assert_eq!(
            app.wallpaper_settings.selection, alternate,
            "首页必须显示用户全局壁纸"
        );
        assert_ne!(
            app.wallpaper_settings.selection,
            WallpaperSelection::default(),
            "不得覆盖成默认地球图"
        );
        app.wallpaper_settings.preferences.global = WallpaperSelection::default();
        app.bind_desktop_wallpaper(cx);
        assert_eq!(
            app.wallpaper_settings.selection,
            WallpaperSelection::default(),
            "恢复默认仍应生效"
        );
    });
}
