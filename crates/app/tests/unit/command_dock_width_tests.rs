use super::*;
use gpui::{px, size, AppContext, Bounds, Pixels, TestAppContext};

/// 场景：恢复底部原 Command Dock 后，项目数量增加也不能挤走输入和设置入口。
///
/// 数据构造：窗口宽 900px，总览先默认收起再手动展开左抽屉（预留最小 366px）；进入项目时不再强制收起左栏；
/// 分别放入 1、4、8 个项目。编号区最多同时显示 4×32+3×4=140px，其余可横向滚动。
/// 执行过程：分别绘制默认总览、展开左栏的总览和进入最后一个项目后的画布，读取 Dock、输入、编号区与设置入口。
/// 预期结果：默认总览 Dock 使用完整画布；左栏展开时总览与项目 Dock 都避开左抽屉；控件不重叠。
#[test]
fn command_dock_fits_canvas_as_projects_increase() {
    for project_count in [1, 4, 8] {
        let (default_overview, overview, project) = command_dock_bounds(project_count, 900.0);
        for dock in [overview, project] {
            assert!(
                dock.left() >= px(384.0),
                "{project_count} 个项目时，左栏展开的 Dock 应避开 366px 左抽屉和 18px 留白：{dock:?}"
            );
        }
        assert!(
            default_overview.left() >= px(18.0) && default_overview.left() < px(300.0),
            "{project_count} 个项目时，左栏收起的默认总览 Dock 应使用完整画布：{default_overview:?}"
        );
        for dock in [default_overview, overview, project] {
            assert!(
                dock.right() <= px(882.0),
                "{project_count} 个项目时，Dock 不能越过右侧 18px 留白"
            );
        }
    }
}

/// 场景：窄窗口切换为覆盖式抽屉时，底部原 Dock 仍完整可见。
///
/// 数据构造：窗口 640×800px；该断点不预留左侧 300px 抽屉宽度；加入 8 个项目。
/// 执行过程：绘制总览和项目画布，检查 Dock、编号区、设置入口及输入边界。
/// 预期结果：Dock 处于窗口两侧各 18px 留白内，编号区至少保留一个 32px 槽位；
/// 输入与设置入口均可见。
#[test]
fn command_dock_fits_narrow_overlay_window() {
    let (default_overview, overview, project) = command_dock_bounds(8, 640.0);
    for dock in [default_overview, overview, project] {
        assert!(dock.left() >= px(18.0), "窄窗口 Dock 不能越过左侧留白");
        assert!(dock.right() <= px(622.0), "窄窗口 Dock 不能越过右侧留白");
    }
}

fn command_dock_bounds(
    project_count: usize,
    window_width: f32,
) -> (Bounds<Pixels>, Bounds<Pixels>, Bounds<Pixels>) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Dock Width").expect("应创建空间");
    let mut last_project = None;
    for index in 0..project_count {
        let root = temp.path().join(format!("project-{index}"));
        std::fs::create_dir(&root).expect("应创建空项目目录");
        last_project = Some(
            catalog
                .add_project(&workspace.id, &root)
                .expect("应添加项目"),
        );
    }
    let last_project = last_project.expect("应至少有一个项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(window_width), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    let default_overview = assert_command_dock_controls_fit(cx, project_count, false);
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.left_open = true;
        app.desktop_workspace.drawer_motion.settle_left(true);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let overview = assert_command_dock_controls_fit(cx, project_count, false);

    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(last_project.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let project = assert_command_dock_controls_fit(cx, project_count, true);
    (default_overview, overview, project)
}

fn assert_command_dock_controls_fit(
    cx: &mut gpui::VisualTestContext,
    project_count: usize,
    project_open: bool,
) -> Bounds<Pixels> {
    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("中央应显示原 Command Dock");
    let input = cx.debug_bounds("command-dock-input").expect("原输入应常驻");
    let switcher = cx
        .debug_bounds("desktop-project-switcher")
        .expect("原 Dock 应显示项目切换入口");
    let settings = cx.debug_bounds("toolbar-settings").expect("应显示设置按钮");
    assert!(
        input.left() >= dock.left() && input.right() <= dock.right(),
        "{project_count} 个项目时原输入不能溢出 Dock"
    );
    if project_open {
        let voice = cx
            .debug_bounds("workspace-voice-control")
            .expect("项目 Dock 应保留原语音入口");
        assert!(input.right() <= voice.left(), "输入与语音入口不能重叠");
        assert!(
            voice.right() <= switcher.left(),
            "语音入口与项目编号不能重叠"
        );
    }
    assert!(
        switcher.size.width >= px(32.0),
        "{project_count} 个项目时项目切换入口不能被裁掉"
    );
    assert!(
        switcher.right() <= settings.left(),
        "{project_count} 个项目时项目编号不能与设置入口重叠"
    );
    assert!(
        settings.right() <= dock.right(),
        "{project_count} 个项目时设置入口不能溢出 Dock"
    );
    dock
}
