use app::AppScreen;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use storage::DesktopWorkspaceStore;

use crate::harness::{echo_factory, make_app};

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("目标控件 {selector} 应可见并带 debug selector"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// DWS-01：首页通过真实控件创建、改名两个空空间，并在原窗口返回目录。
///
/// 数据构造（含关键数值的推导过程）：
///   初始桌面空间数 = 0；点击两次 New Workspace 后 = 0 + 2 = 2。
///   每个新空间未添加项目，因此两个 project_count 都 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless GPUI 启动到工作空间目录，点击 New Workspace → 进入空画布。
///   2. 返回目录，点击第二次 New Workspace → 再进入另一张空画布。
///   3. 返回后点击首张画布的 Rename，输入 Design 并保存。
///   4. 从独立 SQLite 重新读取两个空间。
///
/// 预期结果：
///   - 正断言：两个不同 ID 均持久化，Design 名称存在，画布为空。
///   - 负断言：没有创建旧项目窗口，也没有把第二张画布覆盖第一张。
#[test]
fn desktop_workspace_create_and_rename_from_launchpad() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    assert_eq!(
        app.read_with(cx, |app, _| app.screen()),
        AppScreen::DesktopWorkspace,
        "新建后应进入空工作空间画布"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-add-project").is_some(),
        "空画布应展示添加项目入口"
    );
    click_debug(cx, "desktop-workspace-back");
    click_debug(cx, "desktop-workspace-create");
    assert_eq!(
        app.read_with(cx, |app, _| app.screen()),
        AppScreen::DesktopWorkspace,
        "第二张空画布应可打开"
    );
    click_debug(cx, "desktop-workspace-back");

    click_debug(cx, "desktop-workspace-rename-1");
    assert!(
        cx.debug_bounds("desktop-workspace-rename-dialog").is_some(),
        "点击 Rename 应显示对话框"
    );
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Design");
    click_debug(cx, "desktop-workspace-rename-save");

    let workspaces = store.list().expect("应从 SQLite 恢复新工作空间目录");
    assert_eq!(workspaces.len(), 2, "两次新建应有两条独立记录");
    assert_ne!(workspaces[0].id, workspaces[1].id, "两张空画布不得共享 ID");
    assert!(
        workspaces.iter().any(|item| item.name == "Design"),
        "改名应持久化 Design"
    );
    assert!(
        workspaces.iter().all(|item| item.project_count == 0),
        "新空间不得自动导入旧项目"
    );
    assert_eq!(cx.cx.windows().len(), 1, "所有工作空间操作应留在当前窗口");
}

/// DWS-02：目录选择回调把项目加入当前空画布，重复添加只定位既有区域。
///
/// 数据构造（含关键数值的推导过程）：
///   当前工作空间数 = 1；所选项目目录数 = 1；两次选择同目录后区域数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 点击 New Workspace，进入当前窗口的空画布。
///   2. 通过生产 Project Picker 回调注入一个真实目录。
///   3. 读取画布区域与 SQLite，再次注入同一目录并检查定位到原项目画布。
///
/// 预期结果：
///   - 正断言：首次添加显示区域；重复选择会展开原项目画布，SQLite 中仍仅一条。
///   - 负断言：重复选择不创建第二处区域或旧项目级 project.test.sqlite。
#[test]
fn desktop_workspace_project_picker_adds_one_area_without_legacy_window() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let project = fixture.path().join("project-a");
    std::fs::create_dir(&project).expect("测试应创建项目 A");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取新建工作空间").remove(0);
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("desktop-project-area-0").is_some(),
        "首次加入后总览应显示项目区域"
    );
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx);
        });
    });
    cx.run_until_parked();

    let areas = store
        .list_projects(&workspace.id)
        .expect("应读取当前工作空间区域");
    assert_eq!(areas.len(), 1, "重复选择同目录必须复用当前区域");
    assert_eq!(
        areas[0].root,
        project.canonicalize().expect("项目路径应可规范化"),
        "区域应绑定所选项目目录"
    );
    assert!(
        cx.debug_bounds("desktop-project-expanded").is_some(),
        "重复加入应定位并展开原项目画布"
    );
    click_debug(cx, "desktop-project-back");
    assert!(
        cx.debug_bounds("desktop-project-area-0").is_some(),
        "返回总览后原项目区域应保留"
    );
    assert!(
        cx.debug_bounds("desktop-project-area-1").is_none(),
        "总览不得显示重复区域"
    );
    assert_eq!(cx.cx.windows().len(), 1, "添加项目不应打开第二个项目窗口");
    assert!(
        !project
            .join(".multisoul")
            .join("project.test.sqlite")
            .exists(),
        "添加区域不应加载旧项目画布存档"
    );
}

/// DWS-03：展开项目区域使用该区域的画布，返回总览再展开仍是原会话。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间数 = 1；项目区域数 = 1；初始终端节点数 = 0。
///   展开后创建 Terminal 一张卡片 → 节点数 = 0 + 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空工作空间并添加项目目录。
///   2. 点击项目区域，展开其真实 Canvas；通过测试 PTY 触发 NewTerminal action。
///   3. 返回总览，再点击同一项目区域并读取会话节点数。
///
/// 预期结果：
///   - 正断言：展开时可见 Canvas，返回后总览区域仍在，再展开节点仍为 1。
///   - 负断言：重新展开不生成第二个项目窗口，也不丢失 Terminal 卡片。
#[test]
fn desktop_project_area_expansion_keeps_the_same_canvas_session() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let project = fixture.path().join("project-a");
    std::fs::create_dir(&project).expect("测试应创建项目 A");

    click_debug(cx, "desktop-workspace-create");
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx);
        });
    });
    cx.run_until_parked();
    click_debug(cx, "desktop-project-area-0");
    assert!(
        cx.debug_bounds("desktop-project-expanded").is_some(),
        "点击项目区域应进入展开视图"
    );
    assert!(
        cx.debug_bounds("canvas-surface").is_some(),
        "展开视图应显示真实项目 Canvas"
    );
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(1),
        "新 Terminal 应归属当前项目会话"
    );

    click_debug(cx, "desktop-project-back");
    assert!(
        cx.debug_bounds("desktop-project-area-0").is_some(),
        "返回后总览应保留项目区域"
    );
    click_debug(cx, "desktop-project-area-0");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(1),
        "再展开必须保留原 Terminal 卡片"
    );
    assert_eq!(cx.cx.windows().len(), 1, "展开和返回始终应留在同一窗口");
}

/// DWS-04/DWS-05：展开画布内的快捷键在当前窗口切换到另一工作空间，原卡片保留。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间甲与乙各含项目 A，区域数 = 1 + 1；甲创建终端数 = 0 + 1 = 1；乙仍为 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建甲，添加 A、展开并创建一个测试终端。
///   2. 返回首页创建乙，添加同一 A。
///   3. 在乙按 ⌘⇧K 打开切换器，点击甲，再次展开 A。
///
/// 预期结果：
///   - 正断言：甲的终端仍在，两个区域 ID 不同，始终只有一个窗口。
///   - 负断言：乙没有继承甲的终端，切换器关闭后没有遮挡画布。
#[test]
fn desktop_switcher_keeps_independent_canvas_for_same_project() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let project = fixture.path().join("project-a");
    std::fs::create_dir(&project).expect("测试应创建项目 A");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let first = store.list().expect("应读取甲").remove(0);
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    click_debug(cx, "desktop-project-area-0");
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(1),
        "甲应有一个终端"
    );

    click_debug(cx, "desktop-project-back");
    click_debug(cx, "desktop-workspace-back");
    click_debug(cx, "desktop-workspace-create");
    let second = store
        .list()
        .expect("应读取乙")
        .into_iter()
        .find(|item| item.id != first.id)
        .expect("乙应存在");
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    let first_area = store
        .list_projects(&first.id)
        .expect("应读取甲区域")
        .remove(0);
    let second_area = store
        .list_projects(&second.id)
        .expect("应读取乙区域")
        .remove(0);
    assert_ne!(
        first_area.id, second_area.id,
        "同项目在不同工作空间必须拥有不同区域 ID"
    );
    click_debug(cx, "desktop-project-area-0");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(0),
        "乙不得继承甲的终端"
    );

    cx.simulate_keystrokes("cmd-shift-k");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("desktop-workspace-switcher").is_some(),
        "展开视图快捷键应打开切换器"
    );
    click_debug(cx, "desktop-workspace-switch-1");
    assert!(
        cx.debug_bounds("desktop-workspace-switcher").is_none(),
        "切换后切换器应关闭"
    );
    click_debug(cx, "desktop-project-area-0");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(1),
        "切回甲应保留原终端"
    );
    assert_eq!(cx.cx.windows().len(), 1, "工作空间切换必须留在当前窗口");
}

/// DWS-08：移除项目区域必须先确认，取消保留卡片，确认只删除目标区域。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间数 = 1；区域数 = 1；终端卡片数 = 0 + 1 = 1。
///   取消后仍是 1 区域、1 卡片；确认后为 0 区域，重新加入为 1 个空区域。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建空间和项目区域，展开并创建终端，再返回总览。
///   2. 点击移除并取消，再次展开验证终端继续存在。
///   3. 返回总览、确认移除，重新加入相同项目并展开。
///
/// 预期结果：
///   - 正断言：取消保留原区域和卡片，重新加入产生空画布。
///   - 负断言：确认后旧区域及终端不再出现，项目目录未删除。
#[test]
fn desktop_area_remove_requires_confirmation_and_readd_is_empty() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let project = fixture.path().join("project-a");
    std::fs::create_dir(&project).expect("测试应创建项目 A");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取工作空间").remove(0);
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    let old_area = store
        .list_projects(&workspace.id)
        .expect("应读取原区域")
        .remove(0);
    click_debug(cx, "desktop-project-area-0");
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    click_debug(cx, "desktop-project-back");

    click_debug(cx, "desktop-project-remove-0");
    assert!(
        cx.debug_bounds("desktop-delete-confirmation").is_some(),
        "移除前必须显示确认界面"
    );
    click_debug(cx, "desktop-delete-cancel");
    assert_eq!(
        store
            .list_projects(&workspace.id)
            .expect("取消后应读取区域")
            .len(),
        1,
        "取消不能移除项目区域"
    );
    click_debug(cx, "desktop-project-area-0");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(1),
        "取消不能结束或丢失终端"
    );
    click_debug(cx, "desktop-project-back");

    click_debug(cx, "desktop-project-remove-0");
    click_debug(cx, "desktop-delete-confirm");
    assert!(
        store
            .list_projects(&workspace.id)
            .expect("确认后应读取区域")
            .is_empty(),
        "确认后旧区域必须消失"
    );
    assert!(
        cx.debug_bounds("desktop-project-area-0").is_none(),
        "总览不得残留旧区域"
    );
    assert!(project.is_dir(), "移除区域不能删除项目目录");
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    let new_area = store
        .list_projects(&workspace.id)
        .expect("应读取新区域")
        .remove(0);
    assert_ne!(new_area.id, old_area.id, "重新加入必须创建新区域身份");
    click_debug(cx, "desktop-project-area-0");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_node_count_for_e2e(cx)),
        Some(0),
        "重新加入必须得到空画布"
    );
}

/// DWS-10：首页打开项目必须明确选择桌面工作空间，不能产生旧单项目窗口。
///
/// 数据构造（含关键数值的推导过程）：
///   现有工作空间 = 1；项目目录 = 1；选择后该空间区域数 = 0 + 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建工作空间并返回首页。
///   2. 通过生产项目选择回调选择本地目录。
///   3. 在归属选择器点现有空间，读取 SQLite 与窗口数。
///
/// 预期结果：
///   - 正断言：项目加入选中工作空间并进入总览。
///   - 负断言：选定归属前不创建区域或新窗口，之后也不生成旧项目存档。
#[test]
fn desktop_launchpad_open_project_requires_workspace_destination() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let project = fixture.path().join("project-a");
    std::fs::create_dir(&project).expect("测试应创建项目 A");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取工作空间").remove(0);
    click_debug(cx, "desktop-workspace-back");
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("desktop-project-destination").is_some(),
        "首页选项目后必须选择目标空间"
    );
    assert!(
        store
            .list_projects(&workspace.id)
            .expect("应读取目标区域")
            .is_empty(),
        "选定目标前不得添加区域"
    );
    assert_eq!(cx.cx.windows().len(), 1, "归属选择期间不得新开窗口");

    click_debug(cx, "desktop-project-destination-0");
    assert_eq!(
        store
            .list_projects(&workspace.id)
            .expect("应读取新区域")
            .len(),
        1,
        "项目应归属所选工作空间"
    );
    assert!(
        cx.debug_bounds("desktop-project-area-0").is_some(),
        "选择后应在总览看到项目区域"
    );
    assert!(
        !project
            .join(".multisoul")
            .join("project.test.sqlite")
            .exists(),
        "不得创建旧单项目画布"
    );
}
