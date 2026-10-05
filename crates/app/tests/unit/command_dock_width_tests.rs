use super::*;
use gpui::{px, size, AppContext, Bounds, Pixels, TestAppContext};

/// 场景：项目从 1 个增加到 8 个，快捷指令输入框在总览和项目画布中都保持固定宽度。
///
/// 数据构造（含关键数值的推导过程）：
///   输入框 = 240px，包含左右 8px 内边距与 1px 边框，文本区 = 240 - 2×8 - 2×1 = 222px。
///   项目数 = 1、4、8；编号区上限 = 4×32 + 3×4 = 140px，8 个项目必须滚动访问。
///   窗口 = 900×800px；900px 大于 Dock 上限 720px + 左右 18px 留白 = 756px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别创建 1、4、8 个空项目，绘制真实的总览 Dock 并读取输入框布局边界。
///   2. 展开各组最后一个项目，绘制带语音入口的项目 Dock 并再次读取边界。
///   3. 检查编号区和设置按钮没有与输入框重叠或超出 Dock。
///
/// 预期结果：
///   - 正断言：六个场景的输入框均为 240px，编号和设置入口仍可见。
///   - 负断言：项目增多不能压缩输入框，项目编号不能侵入输入框或挤出设置按钮。
#[test]
fn command_dock_input_keeps_fixed_width_as_projects_increase() {
    let (overview_one, canvas_one) = dock_input_bounds(1);
    let (overview_four, canvas_four) = dock_input_bounds(4);
    let (overview_eight, canvas_eight) = dock_input_bounds(8);

    assert_eq!(
        overview_one.size.width,
        px(240.0),
        "1 个项目的总览输入框应为 240px"
    );
    assert_eq!(
        canvas_one.size.width,
        px(240.0),
        "1 个项目的画布输入框应为 240px"
    );
    assert_eq!(
        overview_four.size.width,
        px(240.0),
        "4 个项目不能压缩总览输入框"
    );
    assert_eq!(
        canvas_four.size.width,
        px(240.0),
        "4 个项目不能压缩画布输入框"
    );
    assert_eq!(
        overview_eight.size.width,
        px(240.0),
        "8 个项目不能压缩总览输入框"
    );
    assert_eq!(
        canvas_eight.size.width,
        px(240.0),
        "8 个项目不能压缩画布输入框"
    );
}

fn dock_input_bounds(project_count: usize) -> (Bounds<Pixels>, Bounds<Pixels>) {
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
    cx.simulate_resize(size(px(900.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    let overview = cx
        .debug_bounds("command-dock-input")
        .expect("总览应显示输入框");

    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(last_project.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let input = cx
        .debug_bounds("command-dock-input")
        .expect("画布应显示输入框");
    let switcher = cx
        .debug_bounds("desktop-project-switcher")
        .expect("应显示项目切换入口");
    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("应显示底栏");
    let settings = cx.debug_bounds("toolbar-settings").expect("应显示设置按钮");
    assert!(
        switcher.left() >= input.right(),
        "{project_count} 个项目的编号不能与输入框重叠"
    );
    assert!(
        settings.left() >= switcher.right(),
        "{project_count} 个项目的编号不能与设置按钮重叠"
    );
    assert!(
        settings.right() <= dock.right(),
        "{project_count} 个项目不能将设置按钮挤出底栏"
    );
    assert!(
        switcher.size.width >= px(32.0),
        "项目切换入口不能被裁成不可见"
    );
    (overview, input)
}
