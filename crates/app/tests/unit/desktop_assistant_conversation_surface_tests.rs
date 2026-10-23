use super::*;
use gpui::{size, AppContext, Modifiers, TestAppContext};

/// 工作空间首次打开时先显示完整画布，Norma 对话保留显式展开入口。
/// 数据构造：空工作空间与 1280×800 的真实 GPUI 视图树。
/// 执行过程：打开工作空间并绘制，再点击左侧展开按钮。
/// 预期结果：首帧左栏不占宽且对话不可见；点击后显示同一对话输入。
#[test]
fn workspace_starts_with_left_drawer_collapsed_and_reopens_on_click() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Closed by default")
        .expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建桌面宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.left_chat_visible = true;
        assert!(
            !app.desktop_workspace.drawers.left_open,
            "工作空间左栏应默认收起"
        );
        assert_eq!(
            app.desktop_workspace.drawer_motion.left.fraction(false),
            0.0,
            "首帧不应绘制一次退出动画"
        );
        assert_eq!(
            app.desktop_assistant_layout(window).left_space,
            0.0,
            "默认收起时左栏不得挤压画布"
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-assistant-chat-input").is_none(),
        "默认状态不能显示左侧对话"
    );
    let reopen = cx
        .debug_bounds("desktop-open-left-drawer")
        .expect("默认收起时应保留展开入口");
    cx.simulate_click(reopen.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-assistant-chat-input").is_some(),
        "用户点击后应能打开 Norma 对话"
    );
}

/// 场景：左抽屉展示当前 Norma 会话的两轮历史。
/// 数据构造：隔离工作空间，分别记录两次用户提问和 Norma 回复。
/// 执行过程：打开工作空间并绘制实际 GPUI 树。
/// 预期结果：第一轮和第二轮消息节点同时存在，未被最近一轮覆盖。
#[test]
fn left_drawer_renders_all_current_session_messages() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Chat History")
        .expect("应创建工作空间");
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
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.left_chat_visible = true;
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
        app.assistant_conversation.user("第一个问题");
        app.assistant_conversation.norma("第一条回复");
        app.assistant_conversation.user("第二个问题");
        app.assistant_conversation.norma("第二条回复");
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    for (id, label) in [
        ("desktop-assistant-message-1", "第一轮用户消息应可见"),
        ("desktop-assistant-message-2", "第一轮 Norma 回复应可见"),
        ("desktop-assistant-message-3", "第二轮用户消息应可见"),
        ("desktop-assistant-message-4", "第二轮 Norma 回复应可见"),
    ] {
        assert!(cx.debug_bounds(id).is_some(), "{label}");
    }
    let user = cx
        .debug_bounds("desktop-assistant-message-3")
        .expect("第二轮用户气泡应有边界");
    let norma = cx
        .debug_bounds("desktop-assistant-message-4")
        .expect("第二轮 Norma 回复应有边界");
    assert!(
        f32::from(user.origin.x) > f32::from(norma.origin.x),
        "用户消息应靠右，Norma 回复应靠左，符合原型的对话层级"
    );
}

/// 场景：左栏的展开/收起只由用户决定，进入与离开项目都不得改写。
/// 数据构造：隔离工作空间内有一个真实项目目录，总览左栏默认收起。
/// 执行过程：展开项目（左栏收起）、在项目内手动打开左栏，再返回工作空间总览。
/// 预期结果：收起的左栏进入项目后保持收起且无滑出动画；项目内仍能按需打开；
///   返回总览时保持用户最后一次选择（展开），不被"恢复"成进入前的状态。
#[test]
fn project_entry_and_exit_never_rewrite_left_drawer_state() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Project Entry").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
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
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        assert!(
            !app.desktop_workspace.drawers.left_open,
            "工作空间总览默认收起左栏"
        );
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        assert!(
            !app.desktop_workspace.drawers.left_open,
            "收起的左栏进入项目后应保持收起"
        );
        assert_eq!(
            app.desktop_workspace.drawer_motion.left.fraction(false),
            0.0,
            "项目首帧不应绘制左栏滑出过程"
        );
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "项目中仍可手动展开 Norma 对话"
        );
        app.collapse_desktop_project_area(window, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "返回总览应保持用户在项目内打开的左栏"
        );
    });
}

/// 场景：用户在总览打开左栏（Run Overview / Chat）后进入 agent 画布，左栏不得被强制收起。
/// 数据构造：窗口 1280×800；展开左栏的目标宽 = 1280×0.31 = 396.8px，画布宽 = 1280 − 396.8 = 883.2px。
/// 执行过程：
///   1. 总览中展开左栏并落稳（motion 展开度 1.0）。
///   2. 展开项目并完成缩放：检查逻辑状态、动效展开度、画布可用宽度。
///   3. 返回总览：再次检查三者。
/// 预期结果：
///   - 进入项目后 `left_open` 仍为 true（旧实现会改为 false）；
///   - 动效展开度仍为 1.0 且仍渲染左栏（旧实现会收起到 0.0）；
///   - 项目内画布宽度仍扣除左栏（约 883.2px），不是整窗 1280px；
///   - 返回总览后三者保持不变。
#[test]
fn project_entry_keeps_open_left_drawer_with_matching_canvas_width() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Open Entry").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
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
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.drawers.left_open = true;
        app.desktop_workspace.drawer_motion.settle_left(true);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "进入项目不得强制收起用户展开的左栏"
        );
        assert_eq!(
            app.desktop_workspace.drawer_motion.left.fraction(false),
            1.0,
            "进入项目后左栏动效应保持完全展开"
        );
        assert!(
            app.desktop_workspace.drawer_motion.left.should_render(),
            "进入项目后左栏仍应被渲染"
        );
        let canvas_width = app.desktop_assistant_layout(window).canvas_width;
        assert!(
            (canvas_width - 883.2).abs() < 1.0,
            "左栏展开时项目画布宽度应扣除左栏（≈883.2px），实际 {canvas_width}"
        );
        app.collapse_desktop_project_area(window, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "返回总览后左栏仍应展开"
        );
        assert_eq!(
            app.desktop_workspace.drawer_motion.left.fraction(false),
            1.0,
            "返回总览后左栏动效仍应完全展开"
        );
    });
}

/// 场景：进入项目后，项目目录在后台加载前不可用。
/// 数据构造：工作空间已保存项目，先手动展开左栏，再在请求展开前移走目录。
/// 执行过程：请求展开并等待后台加载失败。
/// 预期结果：加载期间与失败后左栏始终保持展开；留在工作空间总览并展示加载错误。
#[test]
fn failed_project_entry_keeps_workspace_left_drawer_open() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Missing Project").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
        app.desktop_workspace.drawer_motion.settle_left(true);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        app.desktop_workspace
            .sessions
            .remove(&(area.desktop_workspace_id.clone(), area.id.clone()));
    });
    std::fs::remove_dir(&project_root).expect("应移除项目目录以触发加载失败");
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "加载期间不得收起用户展开的左栏"
        );
    });
    for _ in 0..100 {
        cx.run_until_parked();
        if host.read_with(cx, |app, _| {
            !app.desktop_workspace.loading_area_ids.is_empty()
        }) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        } else {
            break;
        }
    }
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.drawers.left_open,
            "失败后左栏应仍保持展开；expanded={:?} active={:?} pending={} errors={}",
            app.desktop_workspace.expanded_area_id,
            app.desktop_workspace.active_area_id,
            app.desktop_workspace.pending_navigation.is_some(),
            app.desktop_workspace.area_errors.len()
        );
        assert_eq!(
            app.desktop_workspace.drawer_motion.left.fraction(false),
            1.0,
            "失败后左栏动效也应保持完全展开，不能只保留逻辑状态"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "失败后不得保留空项目视图"
        );
        assert!(app.desktop_workspace.error.is_some(), "失败应提供可见错误");
    });
}

/// 场景：项目 A 直接导航到另一个工作空间 B。
/// 数据构造：A 有项目，B 为空，用户在 A 总览手动展开左栏。
/// 执行过程：进入 A 项目，再直接打开 B 并完成导航动效。
/// 预期结果：进入项目后左栏仍展开；B 总览同样展开，不被项目进出改写。
#[test]
fn opening_another_workspace_keeps_left_drawer_state() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace_a = catalog.create("A").expect("应创建 A");
    let workspace_b = catalog.create("B").expect("应创建 B");
    let area = catalog
        .add_project(&workspace_a.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace_a.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.drawers.left_open,
            "进入项目后左栏应保持展开"
        );
        app.open_desktop_workspace(workspace_b.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(workspace_b.id.as_str()),
            "应进入 B"
        );
        assert!(
            app.desktop_workspace.drawers.left_open,
            "B 总览应保持左栏展开"
        );
    });
}

/// 场景：同一工作空间在总览与项目视图之间切换。
/// 数据构造：总览已有一轮对话，项目内继续产生一轮对话。
/// 执行过程：进入项目、手动打开左栏并返回总览。
/// 预期结果：左栏始终使用同一连续历史，两个视图都能看到两轮内容。
#[test]
fn workspace_and_project_share_assistant_conversation_history() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Shared Chat").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.left_chat_visible = true;
        app.assistant_conversation.user("总览中的问题");
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-assistant-message-1").is_some(),
        "项目中应仍显示总览历史"
    );
    let project = host.read_with(cx, |app, _| {
        app.expanded_desktop_project().expect("项目应展开")
    });
    project.update(cx, |app, cx| {
        app.record_assistant_user_message("项目中的问题", cx);
    });
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    host.read_with(cx, |app, _| {
        let messages = app.assistant_conversation.messages();
        assert_eq!(messages.len(), 2, "返回总览后应保留两轮用户消息");
        assert_eq!(messages[1].text, "项目中的问题", "项目会话应追加到同一历史");
    });
}

/// 场景：工作空间 A 的旧加载在 A→B→A 快速导航后才返回。
/// 数据构造：两个隔离工作空间，依次完成三次导航发起。
/// 执行过程：记录每次加载令牌，并比较第一次 A 与第二次 A。
/// 预期结果：每次导航更新令牌，旧 A 回调不能满足新 A 的令牌校验。
#[test]
fn workspace_navigation_invalidates_stale_load_generation() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let a = catalog.create("A").expect("应创建 A");
    let b = catalog.create("B").expect("应创建 B");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    let first = host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.workspace_load_generation
    });
    let second = host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(b.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.workspace_load_generation
    });
    let third = host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.workspace_load_generation
    });
    assert_ne!(first, second, "从 A 到 B 应使 A 的旧回调失效");
    assert_ne!(first, third, "重返 A 不能复用第一次 A 的加载令牌");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(a.id.as_str()),
            "最终应聚焦 A"
        );
    });
}
