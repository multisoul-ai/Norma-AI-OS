use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, Focusable, Modifiers, TestAppContext};

/// 场景：设置输入框的字符键应进入文本通道，不能被面板吞掉或触发画布平移。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库 + 空画布；1280×1000px 窗口为表单预留足够绘制空间。
///   输入 = probe(5字符) + 空格(1字符) + A(1字符) = 7字符。
///   初始说明为空，space_held=false；不启动任何 PTY。
/// 执行过程：
///   1. 打开设置并加载人设 → 聚焦真实 InputState → 按键输入 probe A。
///   2. Cmd+A、Backspace → 清空草稿 → 焦点归还设置面板。
///   3. 面板空白处输入 x、空格、右方向键 → 检查画布没有收到输入。
///   4. Esc → 关闭没有未保存草稿的面板。
///
/// 预期结果：
///   - 正断言：输入值恰为 probe A，随后清空且面板可以关闭。
///   - 负断言：空格不启动画布平移，面板空白输入不写入编辑框或选中卡片。
#[test]
fn settings_text_input_accepts_keystrokes_without_canvas_side_effects() {
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
    cx.simulate_resize(size(px(1280.0), px(1000.0)));
    let app = app.expect("应有 App 实体");
    app.update(cx, |app, _| {
        app.screen = AppScreen::Canvas;
        app.settings_panel.open();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let persona = app.read_with(cx, |app, _| {
        app.norma_persona.as_ref().expect("应加载人设").clone()
    });
    let input = persona.read_with(cx, |view, _| view.instructions.clone());
    cx.update(|window, cx| {
        input.focus_handle(cx).focus(window);
        window.draw(cx).clear();
    });
    cx.simulate_keystrokes("p r o b e space shift-a");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "probe A",
        "字符键必须经平台文本通道写入设置框，不能被面板标记为已消费"
    );
    app.read_with(cx, |app, _| {
        assert!(!app.interaction.space_held, "设置文本空格不得启动画布平移");
    });
    cx.simulate_keystrokes("cmd-a backspace");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "",
        "输入框应保留全选与删除行为"
    );
    app.update_in(cx, |app, window, _| app.focus_handle.focus(window));
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("x space right");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "",
        "焦点在面板空白处时不得继续写入先前的输入框"
    );
    app.read_with(cx, |app, _| {
        assert!(
            !app.interaction.space_held,
            "设置空白处空格不得启动画布平移"
        );
        assert!(
            app.interaction.keyboard_selected_node.is_none(),
            "设置空白处方向键不得选择画布卡片"
        );
        assert!(app.settings_panel.open, "普通按键不得关闭设置");
    });
    cx.simulate_keystrokes("escape");
    app.read_with(cx, |app, _| {
        assert!(!app.settings_panel.open, "Esc 应继续关闭没有草稿的设置");
    });
}

/// 场景：项目设置提升到桌面宿主窗口后，字符键仍进入项目的文本输入框。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离空间1个 + 空项目1个；1280×1000px 窗口；不启动任何 PTY。
///   project(7字符) + 空格(1字符) + 1(1字符) = 9字符。
/// 执行过程：
///   1. 打开空间并展开项目 → 项目打开设置 → 由宿主在窗口层绘制。
///   2. 聚焦项目人设输入框 → 按键输入 project 1 → 检查宿主与项目状态。
///   3. 全选删除 → 聚焦项目设置面板 → Esc关闭。
///
/// 预期结果：
///   - 正断言：项目输入框恰为 project 1；关闭后仍在同一展开项目。
///   - 负断言：宿主不打开第二个设置，项目空格不触发画布平移。
#[test]
fn settings_text_input_in_expanded_project_reaches_platform_text_channel() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建空项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Input").expect("应创建空间");
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
    let title = cx
        .debug_bounds("desktop-home-title-0")
        .expect("应展示隔离空间");
    cx.simulate_click(title.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let area_bounds = cx
        .debug_bounds("desktop-project-area-0")
        .expect("应展示空项目");
    cx.simulate_click(area_bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let project = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    project.update(cx, |app, _| app.settings_panel.open());
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let persona = project.read_with(cx, |app, _| {
        app.norma_persona.as_ref().expect("项目应加载人设").clone()
    });
    let input = persona.read_with(cx, |view, _| view.instructions.clone());
    cx.update(|window, cx| {
        input.focus_handle(cx).focus(window);
        window.draw(cx).clear();
    });
    cx.simulate_keystrokes("p r o j e c t space 1");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "project 1",
        "提升到宿主窗口的设置框不能吞掉项目的普通字符键"
    );
    project.read_with(cx, |app, _| {
        assert!(
            !app.interaction.space_held,
            "项目设置文本空格不得触发画布平移"
        );
    });
    host.read_with(cx, |app, _| {
        assert!(!app.settings_panel.open, "宿主不得打开第二份设置");
    });
    cx.simulate_keystrokes("cmd-a backspace");
    project.update_in(cx, |app, window, _| app.focus_handle.focus(window));
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("escape");
    project.read_with(cx, |app, _| {
        assert!(!app.settings_panel.open, "窗口层项目设置应支持Esc关闭");
    });
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area.id.as_str()),
            "关闭设置不应退出项目"
        );
    });
}
