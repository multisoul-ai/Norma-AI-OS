use crate::harness::{echo_factory, make_app};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use storage::DesktopWorkspaceStore;

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("控件 {selector} 应可见"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// DWS-09：删除含运行任务的工作空间需确认，只停止该空间的运行实例。
///
/// 数据构造（含关键数值的推导过程）：
///   甲、乙各有同一项目的独立区域和 1 个终端；删除甲后活跃实例数从 1 + 1 = 2 降至 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 甲、乙分别加入同一项目，并各启动一个可控 PTY。
///   2. 删除甲时先取消，确认两边仍在运行。
///   3. 再次删除甲并确认，检查乙区域与 PTY、项目目录。
///
/// 预期结果：
///   - 正断言：取消后甲乙都在；确认后乙和项目目录仍在。
///   - 负断言：确认后甲的空间记录与运行实例不再存在，乙的 PTY 未被停止。
#[test]
fn deleting_one_desktop_workspace_preserves_other_workspace_on_same_project() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let project = temp.path().join("shared-project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click(cx, "desktop-workspace-create");
    let first = store.list().expect("应读取甲").remove(0);
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
    click(cx, "desktop-project-area-0");
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    click(cx, "desktop-project-back");
    click(cx, "desktop-workspace-back");

    click(cx, "desktop-workspace-create");
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
    let second_area = store
        .list_projects(&second.id)
        .expect("应读取乙区域")
        .remove(0);
    click(cx, "desktop-project-area-0");
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    click(cx, "desktop-project-back");
    click(cx, "desktop-workspace-back");

    click(cx, "desktop-workspace-delete-1");
    assert!(
        cx.debug_bounds("desktop-delete-confirmation").is_some(),
        "删除甲前应显示影响确认"
    );
    click(cx, "desktop-delete-cancel");
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &first.id,
            &first_area.id,
            cx
        )),
        Some(1),
        "取消后甲的 PTY 应继续运行"
    );
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &second.id,
            &second_area.id,
            cx
        )),
        Some(1),
        "取消后乙的 PTY 应继续运行"
    );

    click(cx, "desktop-workspace-delete-1");
    click(cx, "desktop-delete-confirm");
    assert!(
        store.get(&first.id).expect("应查询甲").is_none(),
        "确认后甲工作空间应消失"
    );
    assert!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &first.id,
            &first_area.id,
            cx
        ))
        .is_none(),
        "甲的运行实例应被移除"
    );
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &second.id,
            &second_area.id,
            cx
        )),
        Some(1),
        "乙的 PTY 不得被甲的删除操作停止"
    );
    assert!(
        store.get(&second.id).expect("应查询乙").is_some(),
        "乙工作空间应保留"
    );
    assert!(project.is_dir(), "删除工作空间不得删除项目目录");
}

/// DWS-04：手机服务保活分支关窗后，Dock 重开复用同一桌面会话与 PTY。
///
/// 数据构造（含关键数值的推导过程）：
///   工作空间 1 个，项目区域 1 个，运行终端 1 个；关窗期间窗口数 = 0，重开后 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在桌面空间启动可控终端并登记当前窗口的桌面 host。
///   2. 关闭窗口，调用手机服务开启时的保活分支。
///   3. 走生产开窗路径重开桌面窗口。
///
/// 预期结果：
///   - 正断言：重开复用同一 App 实体和一个仍在运行的 PTY。
///   - 负断言：后台不依赖隐藏窗口，也没有创建第二个桌面 Runtime。
#[test]
fn desktop_host_reopens_same_runtime_after_background_window_close() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取工作空间").remove(0);
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(project.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    let area = store
        .list_projects(&workspace.id)
        .expect("应读取项目区域")
        .remove(0);
    click(cx, "desktop-project-area-0");
    cx.dispatch_action(app::NewTerminal);
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &workspace.id,
            &area.id,
            cx
        )),
        Some(1),
        "关窗前应有运行 PTY"
    );

    let window = cx.update(|window, _| window.window_handle());
    cx.cx
        .update(|cx| app::MultisoulApp::register_desktop_host_for_e2e(&app, window, cx));
    cx.update(|window, _| window.remove_window());
    cx.cx
        .update(|cx| app::MultisoulApp::desktop_windows_closed_for_e2e(true, cx));
    assert_eq!(
        cx.cx.update(|cx| cx.windows().len()),
        0,
        "后台不得维持隐藏窗口"
    );
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &workspace.id,
            &area.id,
            cx
        )),
        Some(1),
        "保活分支不得停止原 PTY"
    );

    let (reopened, _window) = cx
        .cx
        .update(|cx| app::MultisoulApp::reopen_desktop_window_for_e2e(&app, cx))
        .expect("应通过生产路径重开窗口");
    assert_eq!(
        reopened.entity_id(),
        app.entity_id(),
        "Dock 重开必须复用原桌面 host"
    );
    assert_eq!(
        cx.cx.update(|cx| cx.windows().len()),
        1,
        "重开后应只有一个可见窗口"
    );
    assert_eq!(
        reopened.read_with(cx, |app, cx| app.desktop_area_live_count_for_e2e(
            &workspace.id,
            &area.id,
            cx
        )),
        Some(1),
        "重开后应保留同一 PTY"
    );
    cx.cx.update(app::MultisoulApp::stop_desktop_hosts_for_e2e);
}
