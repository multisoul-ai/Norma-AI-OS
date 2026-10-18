use crate::app::{AppScreen, MultisoulApp};
use gpui::{point, px, size, AppContext, TestAppContext};

/// 首页、工作空间总览与项目画布的顶部栏贴顶铺满可用画布，标题按抽屉状态定位。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个空间 Studio，2 个空目录 project-alpha/project-beta；不创建 PTY 或 Agent。
///   窗口 1280×800；手动展开左栏后目标宽 1280×0.31=396.8px，绘制取整为 397px；中央为 883px。
///   栏高 = 按钮 36px + 上下各 8px = 52px；标题纵向中心 = 52 / 2 = 26px。
///   旧悬浮栏原点 = (18, 8)，中心 y = 8 + 44 / 2 = 30；新标题上移 30 - 26 = 4px。
///   窄窗 640×800，中心 x = 640 / 2 = 320px；容差 1px 覆盖像素取整。
/// 执行过程：
///   1. 打开 Studio 并完成镜头，检查顶部栏贴顶铺满窗口且没有覆盖项目。
///   2. 展开 alpha、切换到 beta，检查项目隐藏左栏后的完整顶部栏，并更新当前项目名称。
///   3. 缩至覆盖模式后检查标题居中，再返回目录并检查首页顶部仍与画布对齐。
///
/// 预期结果：
///   - 正断言：左栏展开时标题靠画布左侧；窄窗覆盖模式居中；alpha/beta 各自在当前项目时出现。
///   - 负断言：顶部栏不能保留悬浮偏移或遮住项目；切换后不残留 alpha；返回目录不残留项目顶部栏。
#[test]
fn desktop_top_bar_tracks_workspace_and_project_with_drawer_alignment() {
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
        app.desktop_workspace.drawers.left_open = true;
        app.desktop_workspace.drawer_motion.settle_left(true);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    let bar = cx
        .debug_bounds("desktop-workspace-top-bar")
        .expect("总览必须有顶部栏");
    assert_eq!(
        bar.size,
        size(px(883.0), px(52.0)),
        "顶部栏必须铺满抽屉之间的画布，并与画布顶部占用空间一致"
    );
    assert_eq!(
        bar.origin,
        point(px(397.0), px(0.0)),
        "顶部栏必须贴合窗口顶边和左抽屉的右边"
    );
    let brand = cx
        .debug_bounds("desktop-assistant-brand")
        .expect("展开的左抽屉必须显示 Norma 标识");
    assert!(
        (brand.center().y - bar.center().y).abs() <= px(1.0),
        "左抽屉的产品标识应与中央顶部栏共用同一条 52px 垂直中心线"
    );
    let left_header = cx
        .debug_bounds("desktop-assistant-left-header")
        .expect("应找到左抽屉标题栏");
    assert!(
        (left_header.bottom() - bar.bottom()).abs() <= px(1.0),
        "左抽屉的底部分隔线应与中央顶部栏对齐"
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
        (title.left() - bar.left() - px(14.0)).abs() <= px(1.0),
        "左抽屉展开时，总览标题应与原型一样贴近中央画布左侧"
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
    // 场景：右抽屉收起时，图标入口仅占用顶栏右侧 32px。
    // 数据构造：1280px 默认左开右关，给工作空间一个长于面包屑可用宽度的名称。
    // 执行过程：读取标题、添加项目按钮和右抽屉入口的绘制边界。
    // 预期结果：长标题停在操作组左侧，操作组与内容入口之间也留有间距。
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.workspaces[0].name = "Studio with a very long workspace name".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let default_long_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("默认态长面包屑应显示");
    let default_add_button = cx
        .debug_bounds("desktop-topbar-add-project")
        .expect("默认态应保留添加项目按钮");
    let default_reader_button = cx
        .debug_bounds("desktop-open-right-drawer")
        .expect("默认态应保留内容浏览器入口");
    assert!(
        default_long_title.right() + px(8.0) <= default_add_button.left(),
        "默认态长面包屑不得压住添加项目按钮"
    );
    assert!(
        default_add_button.right() + px(8.0) <= default_reader_button.left(),
        "添加项目按钮不得压住右抽屉图标入口"
    );
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.workspaces[0].name = "Studio".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    // 场景：同时展开两侧抽屉后，窄中央顶栏采用原型的左对齐面包屑。
    // 数据构造：1280px 窗口、左栏 396.8px、右栏 473.6px，中央 409.6px。
    // 执行过程：打开右侧内容抽屉，绘制后读取标题与顶栏边界，再关闭右栏。
    // 预期结果：标题距中央左边 14px；右栏收起后恢复原有宽栏布局。
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.right_open = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let dual_bar = cx
        .debug_bounds("desktop-workspace-top-bar")
        .expect("双抽屉下中央应保留顶栏");
    let dual_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("双抽屉下应保留项目面包屑");
    assert!(dual_bar.size.width < bar.size.width, "右抽屉应缩窄中央画布");
    assert!(
        (dual_title.left() - dual_bar.left() - px(14.0)).abs() <= px(1.0),
        "双抽屉下标题应像原型一样靠中央左侧"
    );
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.workspaces[0].name = "Studio with a very long workspace name".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let long_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("长标题应保留");
    let add_button = cx
        .debug_bounds("desktop-topbar-add-project")
        .expect("总览仍应能添加项目");
    let workspace_button = cx
        .debug_bounds("workspace-launchpad-button")
        .expect("总览应保留返回工作空间入口");
    assert!(
        long_title.right() + px(8.0) <= add_button.left(),
        "双抽屉长面包屑不得压住右侧操作按钮"
    );
    assert!(
        long_title.right() + px(8.0) <= workspace_button.left(),
        "双抽屉长面包屑不得压住首个返回工作空间按钮"
    );
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.right_open = false;
        app.desktop_workspace.workspaces[0].name = "Studio".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(alpha.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let project_bar = cx
        .debug_bounds("desktop-workspace-top-bar")
        .expect("alpha 项目画布必须保留顶部栏");
    assert_eq!(
        project_bar.origin,
        point(px(0.0), px(0.0)),
        "进入项目后左栏自动收起，顶部栏应从窗口左边开始"
    );
    assert_eq!(
        project_bar.size,
        size(px(1280.0), px(52.0)),
        "进入项目后顶部栏应铺满画布"
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
    // 场景：默认左开右关的真实项目同时有很长的工作空间名和项目名。
    // 数据构造：两个面包屑标签各超过 343px 的默认宽栏单标签上限。
    // 执行过程：重绘长面包屑后读取标题与工作空间入口按钮边界。
    // 预期结果：截断后的标题仍留在右侧操作按钮左边，不能盖住点击目标。
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.workspaces[0].name =
            "Studio long workspace name repeated many times to fill the title".into();
        app.desktop_workspace.areas[0].display_name =
            "Project alpha repeated many times to fill the title".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let project_long_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("项目长面包屑应显示");
    let project_directory_button = cx
        .debug_bounds("workspace-launchpad-button")
        .expect("项目仍可返回工作空间目录");
    assert!(
        project_long_title.right() + px(8.0) <= project_directory_button.left(),
        "默认态两个长名称不得覆盖工作空间入口：标题 {project_long_title:?}，入口 {project_directory_button:?}"
    );
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.right_open = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let narrow_project_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("双抽屉项目仍应显示长面包屑");
    let narrow_project_button = cx
        .debug_bounds("workspace-launchpad-button")
        .expect("双抽屉项目仍应保留返回入口");
    assert!(
        narrow_project_title.right() + px(8.0) <= narrow_project_button.left(),
        "双抽屉下两个长名称不得覆盖返回工作空间按钮：标题 {narrow_project_title:?}，入口 {narrow_project_button:?}"
    );
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.right_open = false;
        app.desktop_workspace.workspaces[0].name = "Studio".into();
        app.desktop_workspace.areas[0].display_name = "project-alpha".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(beta.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("desktop-workspace-top-bar"),
        Some(project_bar),
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
