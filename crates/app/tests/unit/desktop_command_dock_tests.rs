use super::*;
use crate::app::AppScreen;
use gpui::{px, size, AppContext, Modifiers, TestAppContext};
use std::time::Duration;

/// 场景：桌面宿主中的项目 Dock 仍能显示命令建议、处理键盘并返回同窗口目录。
///
/// 数据构造：隔离存储中一个空间、一个空项目；窗口1280×800；/theme包含6个候选。
///   推进220ms等于命令抽屉完整展开时长；初始候选索引0，Down后为1。
/// 执行过程：展开项目 → 输入/theme → 推进抽屉 → Down → Esc → /new codex → 返回目录。
/// 预期结果：建议和键盘有效；浏览建议不创建卡片；异步创建仅生成1张Codex且无假失败；返回不新增窗口。
#[test]
fn desktop_command_dock_keeps_suggestions_and_keyboard_on_the_project() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Commands").expect("应创建空间");
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
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = false;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let project = host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let input = cx
        .debug_bounds("command-dock-input")
        .expect("项目应显示命令输入");
    cx.simulate_click(input.center(), Modifiers::none());
    cx.simulate_input("/theme");
    project.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.command_dock.as_ref().unwrap().draft,
            "/theme",
            "输入应归属项目"
        );
        app.sync_command_drawer(window);
        app.command_dock
            .as_mut()
            .unwrap()
            .drawer
            .tick(Duration::from_millis(220), false);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("command-dock-suggestions").is_some(),
        "宿主必须展示项目的建议面板"
    );
    cx.simulate_keystrokes("down");
    project.update(cx, |app, _| {
        assert_eq!(
            app.command_dock.as_ref().unwrap().selected,
            1,
            "Down应选择第二个候选"
        );
        assert!(
            app.runtime.document.nodes().is_empty(),
            "浏览命令不能创建卡片"
        );
    });
    cx.simulate_keystrokes("escape");
    project.update(cx, |app, _| {
        assert!(
            !app.command_dock.as_ref().unwrap().visible,
            "Esc应关闭项目建议"
        )
    });
    project.update(cx, |app, _| {
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
    });
    cx.simulate_keystrokes("cmd-k cmd-a");
    cx.simulate_input("/new codex");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    project.update(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 1, "异步命令只应创建1张Agent");
        assert!(matches!(&app.runtime.document.nodes()[0].kind, canvas::CanvasNodeKind::Agent(spec) if spec.profile == canvas::AgentProfile::Codex), "必须在所属项目创建Codex");
        let state = app.command_dock.as_ref().unwrap();
        assert!(state.error.is_none(), "已受理的异步创建不能误报失败：{:?}", state.error);
        assert!(state.draft.is_empty(), "命令受理后应清空草稿，防止重复创建");
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let workspace_button = cx
        .debug_bounds("toolbar-workspace")
        .expect("应有工作空间入口");
    cx.simulate_click(workspace_button.center(), Modifiers::none());
    host.update_in(cx, |app, window, cx| {
        // 完成已有镜头及其待处理导航；合并后的目录入口不能另开窗口。
        app.finish_desktop_zoom(window, cx);
        app.finish_desktop_zoom(window, cx);
        assert_eq!(
            app.screen,
            AppScreen::Launchpad,
            "应回到当前窗口工作空间目录"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "目录不能仍展开项目"
        );
        assert_eq!(cx.windows().len(), 1, "工作空间入口不能另开窗口");
    });
}

/// 首页不常驻底部菜单，但无项目时仍可按 Cmd+K 进行个人记忆对话。
///
/// 数据构造（含关键数值的推导过程）：
///   独立空数据根中工作空间数 = 0、项目数 = 0；窗口 1280×800 为桌面基准尺寸。
/// 执行过程（逐步说明系统如何处理）：
///   1. 绘制 Launchpad → 底部菜单和文本输入均隐藏。
///   2. Cmd+K 聚焦 → 按需显示输入；输入明确的中文记忆请求。
///   3. Enter → 尝试提交给 Norma，并反馈缺少模型的错误。
///
/// 预期结果：
///   - 正断言：快捷键后输入可见且保留中文，提交显示真实配置错误。
///   - 负断言：首页无常驻菜单；不伪造项目或会话，也不假报记忆已保存。
#[test]
fn personal_memory_input_is_on_demand_without_a_project() {
    let temp = tempfile::tempdir().expect("应创建独立数据根");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("workspace-command-dock").is_none(),
        "无项目首页不能常驻底部菜单"
    );
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, _| app.focus_handle.focus(window));
    cx.simulate_keystrokes("cmd-k");
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("command-dock-input").is_some(),
        "Cmd+K 后必须显示可用的个人对话输入"
    );
    cx.simulate_input("请记住：我喜欢简洁中文");
    host.update(cx, |app, _| {
        assert_eq!(
            app.command_dock.as_ref().expect("应有文本入口").draft,
            "请记住：我喜欢简洁中文",
            "个人模式中文输入应归属宿主"
        );
        assert!(
            app.workspace_state.current_root.is_none(),
            "文本入口不能伪造项目路径"
        );
        assert!(
            app.agent_runtime.is_none(),
            "仅输入未提交，不应启动 LLM 会话"
        );
    });
    cx.simulate_keystrokes("enter");
    host.update(cx, |app, _| {
        let state = app.command_dock.as_ref().expect("应有文本入口");
        assert!(
            state.error.is_some(),
            "Enter 必须提交给 Norma 并显示配置错误，不能静默返回"
        );
        assert!(
            app.agent_runtime.is_none(),
            "无有效配置不能启动会话或声称已记住"
        );
        assert!(
            app.workspace_state.current_root.is_none(),
            "提交个人对话不能伪造项目"
        );
    });
}
