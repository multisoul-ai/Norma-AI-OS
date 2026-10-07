use super::*;
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

/// 场景：桌面工作区展开某个项目后，点击底栏“布局”按钮必须弹出布局菜单，再点一次收起。
///
/// 数据构造：
///   工作空间 = 1 个，项目 = 1 个空目录；窗口 = 900×800px。
///
/// 执行过程：
///   1. 打开工作空间并展开项目，绘制一帧，此时没有 `layout-menu`。
///   2. 取 `toolbar-layout` 的边界中心并模拟左键点击，再绘制一帧。
///   3. 再次点击同一按钮并绘制。
///
/// 预期结果：
///   - 第一次点击后 `layout-menu` 存在：项目 Dock 的点击翻转了状态，桌面 Dock 必须渲染菜单。
///   - 第二次点击后 `layout-menu` 消失：按钮是开关，菜单不能残留。
#[test]
fn desktop_project_layout_button_toggles_layout_menu() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Layout Menu").expect("应创建空间");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建空项目目录");
    let project = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");

    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(900.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(project.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("layout-menu").is_none(),
        "点击前布局菜单不应显示"
    );

    for (expect_open, step) in [(true, "第一次"), (false, "第二次")] {
        let button = cx
            .debug_bounds("toolbar-layout")
            .expect("项目 Dock 应显示布局按钮");
        cx.simulate_click(button.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            cx.debug_bounds("layout-menu").is_some(),
            expect_open,
            "{step}点击布局按钮后菜单可见性应为 {expect_open}"
        );
    }
}
