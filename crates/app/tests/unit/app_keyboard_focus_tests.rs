use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：没有任何焦点时，语音快捷键仍必须生效，无需先点击卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库 + 空画布；1280×800px 窗口；默认语音快捷键 cmd-e。
///   语音设置默认未就绪（无凭据、无模型），因此快捷键的可观察结果是打开设置面板。
///   `window.blur()` 把 window.focus 置为 None，复现“未聚焦任何卡片”。
/// 执行过程：
///   1. 打开 Canvas，确认设置面板关闭。
///   2. blur 窗口 → 焦点为空，按键分发路径退化为窗口根节点。
///   3. 模拟 cmd-e。
///
/// 预期结果：
///   - 正断言：设置面板被打开（语音快捷键被处理），根焦点句柄已恢复。
///   - 负断言：按键不能因为分发路径不经过 MultisoulApp 根节点而被丢弃。
#[test]
fn test_voice_shortcut_works_when_window_has_no_focus() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let mut test = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let app = app.expect("应有 App 实体");
    app.update(cx, |app, _| {
        app.screen = AppScreen::Canvas;
    });
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        assert!(!app.settings_panel.open, "前置条件：设置面板应处于关闭状态");
    });

    cx.update(|window, cx| {
        window.blur();
        assert!(
            window.focused(cx).is_none(),
            "前置条件：blur 后窗口不应持有任何焦点"
        );
    });

    cx.simulate_keystrokes("cmd-e");

    app.read_with(cx, |app, _| {
        assert!(
            app.settings_panel.open,
            "无焦点时 cmd-e 也必须被处理（语音未就绪应打开设置），不能要求先点击卡片"
        );
    });
    app.update_in(cx, |app, window, cx| {
        assert!(
            app.focus_handle.contains_focused(window, cx),
            "快捷键处理后应恢复根焦点，使后续松键事件能继续送达"
        );
    });
}

/// 场景：桌面空间里展开项目后，键盘焦点落在宿主根节点（而非项目会话）时，Cmd+E 必须由展开的项目处理。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离空间 1 个 + 空项目 1 个；1280×1000px 窗口；不启动 PTY；默认语音快捷键 cmd-e，
///   语音未就绪，可观察结果是“谁打开了设置面板”。
///   宿主在 DesktopWorkspace 屏幕，begin_voice_recording 要求 Canvas 屏幕，所以宿主自己无法录音，
///   只有展开项目会话才是语音的真正承载者。
/// 执行过程：
///   1. 点击空间标题进入空间，再点击项目区域展开项目。
///   2. 把焦点显式移到宿主根句柄（模拟点击顶栏/空白处后焦点留在宿主）。
///   3. 模拟 cmd-e。
///
/// 预期结果：
///   - 正断言：展开项目的设置面板被打开（快捷键被转发给项目会话）。
///   - 负断言：宿主自己的设置面板不得被打开（宿主不能替项目处理语音）。
#[test]
fn test_voice_shortcut_routes_to_expanded_project_when_host_is_focused() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建空项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Voice").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(1000.0)));
    let host = host.expect("应创建宿主");
    host.update(cx, |app, _| app.motion.reduce_motion = true);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let title = cx.debug_bounds("desktop-home-title-0").expect("应展示空间");
    cx.simulate_click(title.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let bounds = cx
        .debug_bounds("desktop-project-area-0")
        .expect("应展示空项目");
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let project = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    host.update_in(cx, |app, window, _| app.focus_handle.focus(window));

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    let project_open = project.read_with(cx, |app, _| app.settings_panel.open);
    let host_open = host.read_with(cx, |app, _| app.settings_panel.open);
    assert!(
        project_open,
        "焦点在宿主时 cmd-e 必须转发给展开项目（语音未就绪应由项目打开设置）"
    );
    assert!(!host_open, "宿主不得自己处理语音快捷键并打开第二份设置");
}
