use super::*;
use gpui::{point, prelude::*, px, size, EntityInputHandler, Focusable, Modifiers, TestAppContext};
use ui::component::input::InputEvent;
use ui::component::Root;

/// 窗口级创建表单只能绘制在宿主层，不能复制到每个真实项目缩略图内。
///
/// 数据构造：一个Root窗口、一个宿主、一个带desktop_host的项目实例；窗口1280×800。
/// 执行过程：打开创建表单 → 单独绘制嵌入项目 → 再绘制窗口宿主。
/// 预期结果：项目画布存在但命名输入不存在；宿主仍显示唯一命名输入。
#[test]
fn desktop_creation_dialog_is_not_repeated_in_project_previews() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let mut test = TestAppContext::single();
    struct PreviewRoot(gpui::AnyView);
    impl gpui::Render for PreviewRoot {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            gpui::div().size_full().child(self.0.clone())
        }
    }
    let mut host = None;
    let mut content = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app =
            cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
        host = Some(app.clone());
        let preview = cx.new(|_| PreviewRoot(app.into()));
        content = Some(preview.clone());
        Root::new(preview, window, cx)
    });
    let content = content.expect("窗口内容存在");
    let host = host.expect("宿主存在");
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_database_context(cx, paths, store);
        app.desktop_host = Some(host.downgrade());
        app.screen = AppScreen::Canvas;
        app
    });
    let viewport = size(px(1280.0), px(800.0));
    cx.simulate_resize(viewport);
    host.update_in(cx, |app, window, cx| {
        app.create_desktop_workspace(window, cx)
    });
    content.update(cx, |preview, cx| {
        preview.0 = session.into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("canvas-surface").is_some(),
        "真实项目内容仍应绘制"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-create-name").is_none(),
        "嵌入项目不能重绘窗口级命名弹窗"
    );
    content.update(cx, |preview, cx| {
        preview.0 = host.into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-create-name").is_some(),
        "宿主的命名弹窗不能一起被隐藏"
    );
}

/// 创建入口必须先收集名称，不能直接写入默认名称空间。
///
/// 数据构造：隔离数据库中 0 个空间，1 个 Root 包裹的桌面窗口，尺寸 1280×800。
///   超长名为 81 个中文字符 = 80 上限 + 1；保存失败使用目录路径代替 SQLite 文件。
/// 执行过程：点击新建 → 检查空输入/焦点 → 空白和超长提交 → IME 组词 Enter → 注入存储失败 → 重试两次。
/// 预期结果：无有效提交时数据库为 0 条；组词不创建；失败保留输入；重试只创建 1 条 trim 后的中文名称。
#[test]
fn desktop_workspace_creation_waits_for_a_name() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (view, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.expect("应创建桌面实例");
    let viewport = size(px(1280.0), px(800.0));
    cx.simulate_resize(viewport);
    cx.run_until_parked();
    cx.draw(point(px(0.0), px(0.0)), viewport, |_, _| {
        gpui::AnyView::from(view.clone()).into_element()
    });
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("应有创建入口");
    cx.simulate_click(create.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        catalog.list().expect("应能读取目录").is_empty(),
        "未输入名称前不能创建 Workspace N 或 New Workspace"
    );
    host.update(cx, |app, _| {
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "命名之前不能跳离当前目录"
        );
        assert!(
            app.desktop_workspace.active_id.is_none(),
            "命名之前不能激活新空间"
        );
    });
    cx.draw(point(px(0.0), px(0.0)), viewport, |_, _| {
        gpui::AnyView::from(view.clone()).into_element()
    });
    assert!(
        cx.debug_bounds("desktop-workspace-create-name").is_some(),
        "点击后应显示名称输入框"
    );
    host.update_in(cx, |app, window, cx| {
        let input = app
            .desktop_workspace
            .create_input
            .clone()
            .expect("应保留命名输入");
        assert!(
            input.read(cx).value().is_empty(),
            "新建名称必须默认空白，不能填入自动名称"
        );
        assert!(
            input.focus_handle(cx).is_focused(window),
            "命名输入应自动聚焦"
        );
        input.update(cx, |input, cx| input.set_value("  ", window, cx));
        app.submit_desktop_workspace_name(window, cx);
        assert!(!app.desktop_workspace.busy, "空白不能开始保存");
        input.update(cx, |input, cx| input.set_value("名".repeat(81), window, cx));
        app.submit_desktop_workspace_name(window, cx);
        assert!(
            !app.desktop_workspace.busy,
            "超过 80 个 Unicode 字符不能开始保存"
        );
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.replace_and_mark_text_in_range(None, "官网", Some(2..2), window, cx);
            cx.emit(InputEvent::PressEnter { secondary: false });
        });
    });
    cx.run_until_parked();
    assert!(
        catalog.list().expect("应读取目录").is_empty(),
        "组词 Enter 只能确认候选，不得创建工作空间"
    );
    host.update_in(cx, |app, window, cx| {
        let input = app
            .desktop_workspace
            .create_input
            .clone()
            .expect("组词时对话框不能消失");
        input.update(cx, |input, cx| {
            input.unmark_text(window, cx);
            input.set_value("  官网改版  ", window, cx);
        });
        app.desktop_workspace.store =
            storage::DesktopWorkspaceStore::new(temp.path().to_path_buf());
        app.submit_desktop_workspace_name(window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(app.desktop_workspace.error.is_some(), "存储失败应显示错误");
        assert!(!app.desktop_workspace.busy, "失败后应解除提交中状态");
        let input = app
            .desktop_workspace
            .create_input
            .as_ref()
            .expect("失败应保留对话框");
        assert_eq!(
            input.read(cx).value().as_ref(),
            "  官网改版  ",
            "失败不能清空用户输入"
        );
        assert!(
            app.desktop_workspace.active_id.is_none(),
            "失败不能导航到不存在的空间"
        );
        app.desktop_workspace.store = catalog.clone();
        app.submit_desktop_workspace_name(window, cx);
        app.submit_desktop_workspace_name(window, cx);
    });
    cx.run_until_parked();
    let saved = catalog.list().expect("应读取保存结果");
    assert_eq!(saved.len(), 1, "连续提交只能创建一个空间");
    assert_eq!(
        saved[0].name, "官网改版",
        "成功名称应去掉首尾空白并保留中文"
    );
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(saved[0].id.as_str()),
            "保存后应进入新空间"
        );
        assert!(
            app.desktop_workspace.create_input.is_none(),
            "成功后应关闭输入"
        );
        assert!(
            app.desktop_workspace.areas.is_empty(),
            "普通新建应进入空项目总览"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "新空间不能继承旧项目展开"
        );
    });
}

/// 添加项目时新建：取消命名保留项目，后续提交只把该项目放入具名空间。
///
/// 数据构造：0 个空间、1 个普通项目目录、来源 Clone；取消之后空间数仍为 0。
/// 执行过程：选择目标为空 → 打开命名 → Escape → 重新命名 → 输入中文后 Enter → 后台添加项目。
/// 预期结果：取消不丢路径/来源且不创建；提交保存 1 个具名空间、1 个正确目录区域，不自动展开。
#[test]
fn desktop_workspace_creation_preserves_pending_project_on_cancel() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (view, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.expect("应创建桌面实例");
    let viewport = size(px(1280.0), px(800.0));
    cx.simulate_resize(viewport);
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.desktop_workspace.pending_project_path = Some(project.clone());
        app.desktop_workspace.pending_project_source = workspace::WorkspaceSource::Clone;
        app.choose_desktop_project_destination(None, window, cx);
    });
    cx.draw(point(px(0.0), px(0.0)), viewport, |_, _| {
        gpui::AnyView::from(view.clone()).into_element()
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(
            app.desktop_workspace.create_input.is_none(),
            "Escape 应关闭命名"
        );
        assert_eq!(
            app.desktop_workspace.pending_project_path.as_ref(),
            Some(&project),
            "取消命名不能丢掉待添加项目"
        );
        assert_eq!(
            app.desktop_workspace.pending_project_source,
            workspace::WorkspaceSource::Clone,
            "取消命名不能把 Clone 来源改为 Open"
        );
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "取消不得离开原目录"
        );
        assert!(
            catalog.list().expect("应读取目录").is_empty(),
            "取消不能写入默认名称空间"
        );
        app.choose_desktop_project_destination(None, window, cx);
        let input = app
            .desktop_workspace
            .create_input
            .clone()
            .expect("应再次显示命名框");
        input.update(cx, |input, cx| input.set_value("项目验收", window, cx));
    });
    cx.draw(point(px(0.0), px(0.0)), viewport, |_, _| {
        gpui::AnyView::from(view.clone()).into_element()
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let saved = catalog.list().expect("应保存新空间");
    assert_eq!(
        saved.len(),
        1,
        "Enter 只能创建一次，不能与 Dialog confirm 重复提交"
    );
    assert_eq!(saved[0].name, "项目验收", "必须保存用户名称");
    let areas = catalog.list_projects(&saved[0].id).expect("应加载项目区域");
    assert_eq!(areas.len(), 1, "创建后应接续原项目添加流程");
    assert_eq!(
        areas[0].root,
        project.canonicalize().expect("应解析项目目录"),
        "区域必须使用原待添加目录"
    );
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.pending_project_path.is_none(),
            "成功后应清理目标选择器"
        );
        assert!(
            app.desktop_workspace.create_input.is_none(),
            "成功后应关闭命名"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "添加后应停在项目总览"
        );
    });
}

/// 工作空间列表在打开、返回、搜索和新建之后维持创建顺序，与重新加载的目录一致。
///
/// 数据构造：先后创建Alpha/Beta共2个空空间，新建Gamma后2+1=3；窗口1280×800。
/// 执行过程：打开Beta再返回 → 搜索Beta并清空 → 命名新建Gamma → 重新加载目录。
/// 预期结果：Alpha始终第一、Beta始终第二；Gamma追加末尾；搜索不改原列表、最近打开不置顶。
#[test]
fn desktop_workspace_order_stays_stable_after_navigation_and_creation() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let alpha = catalog.create("Alpha").unwrap();
    let beta = catalog.create("Beta").unwrap();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.unwrap();
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(beta.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.show_desktop_launchpad(window, cx);
        assert_eq!(
            app.desktop_workspace.workspaces[0].id, alpha.id,
            "打开Beta不能将它放到首位"
        );
        assert_eq!(
            app.desktop_workspace.workspaces[1].id, beta.id,
            "Beta应留在原第二位"
        );
        app.desktop_workspace.search_query = "Beta".into();
        assert_eq!(
            app.desktop_workspace.filtered()[0].id,
            beta.id,
            "搜索应仍能找到Beta"
        );
        app.desktop_workspace.search_query.clear();
        assert_eq!(
            app.desktop_workspace.filtered()[0].id,
            alpha.id,
            "清空搜索后顺序不能重排"
        );
    });
    // Let the directory refresh settle before checking the creation callback's own ordering.
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.create_desktop_workspace(window, cx);
        let input = app.desktop_workspace.create_input.clone().unwrap();
        input.update(cx, |input, cx| input.set_value("Gamma", window, cx));
        app.submit_desktop_workspace_name(window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.workspaces.len(),
            3,
            "新建后应有三个空间"
        );
        assert_eq!(
            app.desktop_workspace.workspaces[0].id, alpha.id,
            "新建不得挤走Alpha"
        );
        assert_eq!(
            app.desktop_workspace.workspaces[1].id, beta.id,
            "新建不得挤走Beta"
        );
        assert_eq!(
            app.desktop_workspace.workspaces[2].name, "Gamma",
            "新建Gamma应追加末尾"
        );
        assert_ne!(
            app.desktop_workspace.workspaces[0].name, "Gamma",
            "不能在内存中把新空间插到头部"
        );
        assert_eq!(
            app.desktop_workspace.workspaces,
            catalog.list().unwrap(),
            "内存顺序必须与重载后的数据库一致"
        );
    });
}
