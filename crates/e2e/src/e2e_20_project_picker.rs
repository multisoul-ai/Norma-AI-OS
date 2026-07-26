use gpui::TestAppContext;

use crate::harness::{echo_factory, make_app};

/// Open Project：组件 Root 窗口收到单目录选择结果后必须打开测试项目。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口数 = 1（Launchpad）。
///   选择目录数 = 1（临时 workspace）。
///   成功后的窗口数 = 1（初始 1 - 关闭 Launchpad 1 + 新项目窗口 1）。
///   测试项目数据库数 = 1（project.test.sqlite）。
///   生产项目数据库数 = 0（project.sqlite 不得创建）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app` 创建由 gpui-component Root 包裹的真实 Launchpad App。
///   2. 通过测试入口向生产 Project Picker 回调注入一个目录选择结果。
///   3. 推进 GPUI executor，回调通过窗口上下文创建项目窗口并关闭 Launchpad。
///   4. 从隔离 app DB 读取 Recent，并检查 workspace 内的项目数据库文件。
///
/// 预期结果：
///   - 正断言：Recent 中存在所选 canonical workspace。
///   - 正断言：workspace 创建 `project.test.sqlite` 且只保留一个项目窗口。
///   - 负断言：Recent 中不存在未选择目录，且不得创建 `project.sqlite`。
#[test]
fn open_project_selection_enters_workspace_from_component_root() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("Open Project E2E 应创建隔离 fixture");
    let workspace = fixture.path().join("selected-workspace");
    let unselected_workspace = fixture.path().join("unselected-workspace");
    std::fs::create_dir(&workspace).expect("Open Project E2E 应创建所选 workspace");
    std::fs::create_dir(&unselected_workspace).expect("Open Project E2E 应创建负对照 workspace");
    let app_database_path = app.read_with(cx, |app, _cx| app.app_database_path_for_e2e());

    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.complete_project_picker_for_e2e(workspace.clone(), window, cx);
        });
    });
    cx.run_until_parked();

    let canonical_workspace = workspace
        .canonicalize()
        .expect("所选 workspace 应可 canonicalize");
    let canonical_unselected = unselected_workspace
        .canonicalize()
        .expect("负对照 workspace 应可 canonicalize");
    let recent = storage::RecentStore::new(app_database_path)
        .load()
        .expect("Open Project E2E 应从隔离 app DB 读取 Recent");
    assert!(
        recent
            .records
            .iter()
            .any(|record| record.path == canonical_workspace),
        "Project Picker 回调完成后 Recent 必须包含所选 canonical workspace"
    );
    assert!(
        workspace
            .join(".multisoul")
            .join("project.test.sqlite")
            .is_file(),
        "Open Project 在测试模式必须创建 project.test.sqlite"
    );
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "Open Project 成功后应关闭 Launchpad，仅保留一个项目窗口"
    );
    assert!(
        !recent
            .records
            .iter()
            .any(|record| record.path == canonical_unselected),
        "Recent 不得误记录未选择的负对照 workspace"
    );
    assert!(
        !workspace.join(".multisoul").join("project.sqlite").exists(),
        "Open Project 测试路径不得创建生产 project.sqlite"
    );
}
