use crate::app::{AppScreen, MultisoulApp};
use gpui::{point, px, size, AppContext, TestAppContext};

/// 首页、工作空间总览与项目画布的顶部栏贴顶铺满窗口，标题居中且随项目切换更新。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个空间 Studio，2 个空目录 project-alpha/project-beta；不创建 PTY 或 Agent。
///   窗口 1280×800；顶部原点 = (0, 0)，栏宽 = 窗口宽 1280px。
///   栏高 = 按钮 36px + 上下各 8px = 52px；标题中心 = (1280 / 2, 52 / 2) = (640, 26)。
///   旧悬浮栏原点 = (18, 8)，中心 y = 8 + 44 / 2 = 30；新标题上移 30 - 26 = 4px。
///   窄窗 640×800，中心 x = 640 / 2 = 320px；容差 1px 覆盖像素取整。
/// 执行过程：
///   1. 打开 Studio 并完成镜头，检查顶部栏贴顶铺满窗口且没有覆盖项目。
///   2. 展开 alpha、切换到 beta，检查顶部栏边界与总览一致，并更新当前项目名称。
///   3. 缩窗后检查标题居中，再返回目录并检查首页顶部仍与画布对齐。
/// 预期结果：
///   - 正断言：总览和项目顶部栏均贴顶全宽、名称居中；alpha/beta 各自在当前项目时出现。
///   - 负断言：顶部栏不能保留悬浮偏移或遮住项目；切换后不残留 alpha；返回目录不残留项目顶部栏。
#[test]
fn desktop_top_bar_tracks_workspace_and_project_at_window_center() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let alpha_root = temp.path().join("project-alpha");
    let beta_root = temp.path().join("project-beta");
    std::fs::create_dir(&alpha_root).expect("应创建 alpha 空项目");
    std::fs::create_dir(&beta_root).expect("应创建 beta 空项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Studio").expect("应创建 Studio");
    let alpha = catalog
        .add_project(&workspace.id, &alpha_root)
        .expect("应添加 alpha");
    let beta = catalog
        .add_project(&workspace.id, &beta_root)
        .expect("应添加 beta");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    let bar = cx
        .debug_bounds("desktop-workspace-top-bar")
        .expect("总览必须有顶部栏");
    assert_eq!(
        bar.size,
        size(px(1280.0), px(52.0)),
        "顶部栏必须铺满窗口宽度，并与原有画布顶部占用空间一致"
    );
    assert_eq!(
        bar.origin,
        point(px(0.0), px(0.0)),
        "顶部栏必须贴合窗口顶边和左边"
    );
    assert_ne!(
        bar.origin,
        point(px(18.0), px(8.0)),
        "顶部栏不能保留旧悬浮栏的左右和顶部留白"
    );
    let first_project = cx
        .debug_bounds("desktop-project-area-0")
        .expect("总览应显示 alpha 项目");
    assert!(
        first_project.top() >= bar.bottom() + px(12.0),
        "项目卡片必须避开顶部栏并留出12px间距"
    );
    let title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("应显示居中名称");
    assert!(
        (title.center().x - px(640.0)).abs() <= px(1.0),
        "总览标题应对齐窗口中心"
    );
    assert!(
        (title.center().y - px(26.0)).abs() <= px(1.0),
        "总览标题应随顶部栏上移，居中于 52px 顶栏"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-workspace-Studio")
            .is_some(),
        "总览必须显示 Studio"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-project-project-alpha")
            .is_none(),
        "总览不能显示 alpha 为当前项目"
    );
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(alpha.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("desktop-workspace-top-bar"),
        Some(bar),
        "alpha 项目画布的顶部栏必须与工作空间总览保持相同位置和尺寸"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-project-project-alpha")
            .is_some(),
        "展开 alpha 必须显示其名称"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-project-project-beta")
            .is_none(),
        "展开 alpha 不能显示 beta"
    );
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(beta.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("desktop-workspace-top-bar"),
        Some(bar),
        "切换到 beta 后顶部栏不能产生额外偏移或改变宽度"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-project-project-beta")
            .is_some(),
        "切换后必须显示 beta"
    );
    assert!(
        cx.debug_bounds("workspace-topbar-project-project-alpha")
            .is_none(),
        "切换后不能残留 alpha 名称"
    );
    cx.simulate_resize(size(px(640.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("窄窗应保留标题");
    let narrow_bar = cx
        .debug_bounds("desktop-workspace-top-bar")
        .expect("窄窗必须保留顶部栏");
    assert_eq!(
        narrow_bar.size,
        size(px(640.0), px(52.0)),
        "窄窗顶部栏仍须铺满 640px 窗口，控件不能溢出窗口"
    );
    assert!(
        (title.center().x - px(320.0)).abs() <= px(1.0),
        "窄窗标题仍须对齐窗口中心"
    );
    assert!(
        title.left() >= px(88.0) && title.right() <= px(552.0),
        "窄窗标题不能覆盖两侧控件"
    );
    host.update_in(cx, |app, window, cx| {
        app.show_desktop_launchpad(window, cx);
        app.finish_desktop_zoom(window, cx);
        assert_eq!(app.screen, AppScreen::Launchpad, "返回目录必须留在同一窗口");
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-top-bar").is_none(),
        "目录不能叠加项目顶部栏"
    );
    assert_eq!(
        cx.debug_bounds("desktop-home-toolbar"),
        Some(narrow_bar),
        "首页顶部栏必须与工作空间和项目画布保持相同位置、宽度和高度"
    );
}
