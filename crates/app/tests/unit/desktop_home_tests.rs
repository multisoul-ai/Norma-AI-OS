use super::*;
use crate::app::AppScreen;
use gpui::{size, AppContext, Modifiers, TestAppContext};

/// 首次启动的空目录提供居中的创建入口，取消无副作用，创建后退出欢迎态。
///
/// 数据构造：隔离数据库含0个工作空间；窗口1280×800，工具栏52px，内容中心y=(52+800)/2=426；
///   窄窗360×480，左右至少24px；几何容差1px来自GPUI像素取整。浅色主题覆盖空态字体与布局。
/// 执行过程：启动空目录 → 检查欢迎态及唯一创建入口 → 缩窗/换主题 → 创建弹窗取消
///   → 再次创建并命名 → 返回首页 → 搜索不存在名称。
/// 预期结果：不显示空列表搜索和继续工作分组；窄窗标题/按钮不截断；取消不写入，
///   首次创建仅写入1个空工作空间；无搜索结果不冒充首次启动欢迎页。
#[test]
fn desktop_home_first_workspace_welcome_and_creation() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
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
    cx.update(|window, cx| window.draw(cx).clear());
    let welcome = cx
        .debug_bounds("desktop-home-welcome")
        .expect("空目录应显示欢迎面板");
    assert!(
        (welcome.center().x - px(640.0)).abs() <= px(1.0),
        "欢迎面板应水平居中"
    );
    assert!(
        (welcome.center().y - px(426.0)).abs() <= px(1.0),
        "欢迎面板应居中于工具栏下方，不能挤在左上角"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-search").is_none(),
        "无工作空间时不能显示无用搜索"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-scroll").is_none(),
        "欢迎态不能沿用空列表滚动及分组布局"
    );
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("欢迎态必须有创建入口");
    assert!(
        welcome.contains(&create.center()),
        "创建按钮应在欢迎面板内，不能只留在工具栏"
    );
    assert_eq!(
        cx.debug_bounds("desktop-home-wallpaper")
            .expect("欢迎态复用设置壁纸")
            .size,
        size(px(1280.0), px(800.0)),
        "壁纸应覆盖整个窗口"
    );
    host.update(cx, |app, cx| {
        app.theme_id = ui::ThemeId::InkBlack;
        app.theme = ui::Theme::for_id(app.theme_id);
        app.motion.reduce_motion = true;
        cx.notify();
    });
    cx.simulate_resize(size(px(360.0), px(480.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let welcome = cx
        .debug_bounds("desktop-home-welcome")
        .expect("窄窗仍显示欢迎面板");
    let title = cx
        .debug_bounds("desktop-home-welcome-title")
        .expect("应显示欢迎标题");
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("窄窗应保留创建按钮");
    assert!(
        welcome.left() >= px(24.0) && welcome.right() <= px(336.0),
        "360px窄窗左右各保留至少24px"
    );
    assert!(
        welcome.top() >= px(52.0) && welcome.bottom() <= px(480.0),
        "欢迎内容不能覆盖工具栏或越出底部"
    );
    assert!(
        title.left() >= welcome.left() && title.right() <= welcome.right(),
        "标题应随窄窗换行，不能撑破面板"
    );
    assert!(
        create.top() > title.bottom() && create.bottom() <= welcome.bottom(),
        "创建按钮不能与标题重叠或被裁断"
    );
    cx.simulate_click(create.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    let cancel = cx
        .debug_bounds("desktop-workspace-create-cancel")
        .expect("应打开原有命名弹窗");
    cx.simulate_click(cancel.center(), Modifiers::none());
    assert!(
        catalog.list().expect("读取目录").is_empty(),
        "取消不能创建默认命名的工作空间"
    );
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_click(create.center(), Modifiers::none());
    cx.simulate_input("My first workspace");
    cx.update(|window, cx| window.draw(cx).clear());
    let confirm = cx
        .debug_bounds("desktop-workspace-create-confirm")
        .expect("应显示创建确认按钮");
    cx.simulate_click(confirm.center(), Modifiers::none());
    cx.run_until_parked();
    let created = catalog.list().expect("读取创建结果");
    assert_eq!(created.len(), 1, "只应创建一个工作空间");
    assert_eq!(
        created[0].name, "My first workspace",
        "必须保留用户输入名称"
    );
    assert_eq!(
        created[0].project_count, 0,
        "首次创建仍从空画布开始，不能塞入示例项目"
    );
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.screen,
            AppScreen::DesktopWorkspace,
            "创建成功后应进入新工作空间"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "不能跳入不存在的项目"
        );
        app.show_desktop_launchpad(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-card-0").is_some(),
        "返回首页应展示刚创建的工作空间"
    );
    assert!(
        cx.debug_bounds("desktop-home-welcome").is_none(),
        "创建成功后不能继续显示首次欢迎态"
    );
    let search = cx
        .debug_bounds("desktop-workspace-search")
        .expect("应有搜索");
    cx.simulate_click(search.center(), Modifiers::none());
    cx.simulate_input("missing workspace");
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-search").is_some(),
        "搜索无结果时应保留搜索入口"
    );
    assert!(
        cx.debug_bounds("desktop-home-welcome").is_none(),
        "搜索无结果不能显示首次创建欢迎态"
    );
    assert!(
        cx.debug_bounds("desktop-home-search-empty").is_some(),
        "搜索无结果应显示独立空态，最近访问留在背后"
    );
}

/// 场景：首页固定四张最近卡，搜索原位展开且键盘打开正确空间。
/// 数据：12个空空间，1440×900；标准卡252×204，搜索640×56。
/// 执行：绘制→点击搜索→输入Workspace 11→下/回车；再返回首页。
/// 预期：四张等宽卡；无旧标题/主按钮；面板不挪动搜索；回车仅打开匹配项。
#[test]
fn desktop_home_combo_search_and_keyboard_open() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let mut target = String::new();
    for i in 0..12 {
        let workspace = catalog
            .create(&format!("Workspace {i:02}"))
            .expect("应创建空间");
        if i == 0 {
            target = workspace.id;
        }
    }
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应有宿主");
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.motion.reduce_motion = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    for selector in [
        "desktop-home-card-0",
        "desktop-home-card-1",
        "desktop-home-card-2",
        "desktop-home-card-3",
    ] {
        let card = cx.debug_bounds(selector).expect("应显示最近卡片");
        assert_eq!(
            card.size,
            size(px(252.0), px(204.0)),
            "所有最近卡片使用标准等宽尺寸"
        );
    }
    assert!(
        cx.debug_bounds("desktop-home-card-4").is_none(),
        "首页不能堆砌全部12个空间"
    );
    assert!(
        cx.debug_bounds("desktop-home-heading").is_none(),
        "旧大标题必须移除"
    );
    assert!(
        cx.debug_bounds("desktop-home-open").is_none(),
        "移除独立打开按钮，整卡打开"
    );
    let search = cx
        .debug_bounds("desktop-workspace-search")
        .expect("应显示搜索胶囊");
    assert_eq!(search.size.width, px(640.0), "搜索宽640px");
    assert!(
        cx.debug_bounds("desktop-home-search-results").is_none(),
        "静止不显示搜索面板"
    );
    cx.simulate_click(search.center(), Modifiers::none());
    cx.simulate_input("Workspace 00");
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-search-results").is_some(),
        "聚焦与输入展开搜索"
    );
    assert_eq!(
        cx.debug_bounds("desktop-workspace-search")
            .expect("搜索应保留")
            .origin,
        search.origin,
        "展开原位向下生长"
    );
    assert!(
        cx.debug_bounds("desktop-home-result-0").is_some(),
        "应显示匹配结果"
    );
    assert!(
        cx.debug_bounds("desktop-home-result-1").is_none(),
        "不能保留不匹配的结果"
    );
    cx.simulate_keystrokes("down enter");
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.transition.is_none(),
            "非最近卡没有可见来源，应直接进入"
        )
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(target.as_str()),
            "回车打开搜索选中的空间"
        );
        assert_eq!(
            app.screen,
            AppScreen::DesktopWorkspace,
            "确认后进入工作空间"
        );
    });
}
