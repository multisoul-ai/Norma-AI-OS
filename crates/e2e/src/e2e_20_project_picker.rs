use gpui::TestAppContext;

use crate::harness::{echo_factory, make_app};

/// 项目加载失败后应在原启动页显示原因，修复数据后同一路径可重试成功。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口 = 1 个启动页；失败窗口应关闭，失败后仍为 1 个原窗口。
///   项目快照先写入非法 JSON "{"，随后恢复 1 张 Rect(id=7) 的合法快照。
///   重试成功后窗口 = 原启动页 1 - 关闭 1 + 项目窗口 1 = 1。
///
/// 执行过程：
///   1. 在隔离项目写入损坏快照，通过生产目录选择回调打开项目。
///   2. 检查原启动页显示快照加载错误，失败窗口不残留、不替换原窗口。
///   3. 恢复合法快照，再次通过同一回调选择项目，读取最终窗口的画布状态。
///
/// 预期结果：
///   - 正断言：失败原因可读；重试后进入 Canvas 并恢复 Rect(7)。
///   - 负断言：失败不得关闭原启动页；重试不得命中假成功窗口或遗留错误。
#[test]
fn open_project_failure_keeps_launchpad_and_allows_retry() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("应创建隔离项目");
    let store = crate::harness::project_store(&app, cx, fixture.path());
    // 100×80 沿用布局测试尺寸；Rect 没有 PTY 或会话副作用。
    let snapshot = canvas::DocumentSnapshot {
        nodes: vec![canvas::CanvasNodeSnapshot {
            id: 7,
            kind: canvas::CanvasNodeKind::Rect(canvas::RectNode { color: 0x4080ff }),
            bounds: canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 1,
        }],
        viewport: canvas::Viewport::default(),
    };
    store
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("应建立测试快照");
    let connection =
        rusqlite::Connection::open(fixture.path().join(".multisoul/project.test.sqlite"))
            .expect("应打开隔离测试库");
    connection
        .execute("UPDATE canvas_snapshots SET json = '{' WHERE id = 1", [])
        .expect("应注入 JSON 加载失败");
    drop(connection);
    let original_window = cx.update(|window, _| window.window_handle());
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.complete_project_picker_for_e2e(fixture.path().into(), window, cx);
        })
    });
    cx.run_until_parked();
    let error = app.read_with(cx, |app, _| app.workspace_error_for_e2e());
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("Could not restore canvas snapshot")),
        "启动页必须显示具体加载错误，实际为 {error:?}"
    );
    assert!(
        cx.cx.windows() == vec![original_window],
        "失败窗口应关闭，原启动页必须保留"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.screen()),
        app::AppScreen::Launchpad,
        "失败后原窗口必须仍处于可重试的启动页"
    );

    store
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("应恢复合法测试数据");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.complete_project_picker_for_e2e(fixture.path().into(), window, cx);
        })
    });
    cx.run_until_parked();
    let windows = cx.cx.windows();
    assert_eq!(windows.len(), 1, "重试成功后只能保留一个项目窗口");
    assert!(!windows.contains(&original_window), "成功后原启动页应关闭");
    let opened = cx.cx.update(|cx| {
        windows[0]
            .downcast::<ui::component::Root>()
            .expect("项目应使用组件 Root")
            .read(cx)
            .expect("项目窗口应可读")
            .view()
            .clone()
            .downcast::<app::MultisoulApp>()
            .expect("Root 应承载真实 App")
    });
    opened.read_with(cx, |opened, _| {
        assert_eq!(
            opened.screen(),
            app::AppScreen::Canvas,
            "重试必须实际进入画布"
        );
        assert!(
            opened.runtime.document.node(7).is_some(),
            "必须恢复修复后的 Rect(7)"
        );
        assert!(
            opened.runtime.document.node(8).is_none(),
            "不得额外创建 Rect(8)"
        );
        assert!(
            opened.workspace_error_for_e2e().is_none(),
            "重试成功不得遗留加载错误"
        );
    });
}

/// 手机端卡片在内置名称用尽后重新打开，必须保留标题与未绑定身份并进入画布。
///
/// 数据构造（含关键数值的推导过程）：
///   定义数 = 内置名称 30 个；可用内置名称 = 30 - 30 = 0。
///   手机端卡片 = 1 张 Claude，标题 ui优化（手机端自由标题），definition_id=None。
///   不推进模拟时钟，仅验证同步加载，不执行延后的真实 PTY 恢复。
///
/// 执行过程：
///   1. 在隔离项目写满内置定义，通过 Runtime 手机端 API 创建并保存卡片。
///   2. 调用生产项目窗口打开流程，从磁盘恢复项目。
///   3. 检查画布、卡片身份、持久化快照与定义数量。
///
/// 预期结果：
///   - 正断言：进入 Canvas，原卡片及 ui优化 标题仍存在，定义总数仍为 30。
///   - 负断言：不得停留 Launchpad、绑定手机卡片、创建迁移定义或显示加载错误。
#[test]
fn open_project_preserves_mobile_card_with_exhausted_builtin_names() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("应创建隔离项目");
    let store = crate::harness::project_store(&app, cx, fixture.path());
    for name in agent::builtin_agent_names() {
        store
            .agent_definition_store()
            .create(&storage::NewAgentDefinition {
                name: name.name().to_string(),
                runtime_profile: canvas::AgentProfile::Codex,
                workflow_description: String::new(),
            })
            .expect("应填满内置名称池");
    }
    let mut runtime = runtime::Runtime::new();
    // 640×400 沿用卡片测试尺寸；尺寸不影响身份迁移。
    let node = runtime
        .create_mobile_agent_card(
            canvas::AgentProfile::Claude,
            None,
            Some("ui优化".into()),
            canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("手机端应允许自由标题和空 definition_id");
    runtime
        .save_canvas_snapshot(&store.canvas_store())
        .expect("应保存手机卡片");

    let (opened, _) = cx.update(|_, cx| {
        app::MultisoulApp::reopen_project_window_for_e2e(&app, fixture.path().into(), cx)
            .expect("应创建项目窗口")
    });
    opened.read_with(cx, |opened, _| {
        assert_eq!(opened.screen(), app::AppScreen::Canvas, "名称池用尽不得阻止手机项目进入画布");
        assert_ne!(opened.screen(), app::AppScreen::Launchpad, "项目打开后不能仍是启动页");
        assert!(opened.workspace_error_for_e2e().is_none(), "手机卡片不得产生旧卡迁移错误");
        let card = opened.runtime.document.node(node).expect("原手机卡片必须恢复");
        assert!(matches!(&card.kind, canvas::CanvasNodeKind::Agent(spec)
            if spec.mobile_control && spec.definition_id.is_none() && spec.alias.as_deref() == Some("ui优化")),
            "手机卡片的标题、模式和未绑定身份都必须保留");
    });
    assert_eq!(
        store
            .agent_definition_store()
            .list()
            .expect("应读取定义")
            .len(),
        30,
        "手机卡片重新打开不得创建任何迁移定义"
    );
    let saved = store
        .canvas_store()
        .load_snapshot()
        .expect("应读取快照")
        .expect("快照应存在");
    assert_eq!(saved.nodes.len(), 1, "不得增删持久化卡片");
    assert!(
        matches!(&saved.nodes[0].kind, canvas::CanvasNodeKind::Agent(spec)
        if spec.definition_id.is_none() && spec.alias.as_deref() == Some("ui优化")),
        "磁盘卡片不得被迁移改名或绑定"
    );
}

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
