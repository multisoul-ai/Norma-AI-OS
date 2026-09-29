use crate::harness::{echo_factory, make_app};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use storage::DesktopWorkspaceStore;

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("目标控件 {selector} 应可见并带 debug selector"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// DWS-03/DWS-11：重新打开双项目工作空间时两张区域会话加载，缺失项目仅标记不可用。
///
/// 数据构造（含关键数值的推导过程）：
///   项目目录 A、B 各 1 个；区域数 = 1 + 1 = 2；移走 B 后可加载会话数 = 2 - 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建工作空间并添加 A、B，返回首页后重新打开。
///   2. 核对两个区域均加载为可操作会话。
///   3. 再回首页移走 B，然后重新打开同一工作空间。
///
/// 预期结果：
///   - 正断言：首次打开加载两张区域，B 缺失后仍显示其区域和不可用提示。
///   - 负断言：B 缺失不阻断 A 的会话或删除项目区域。
#[test]
fn desktop_overview_loads_area_sessions_and_keeps_missing_project_visible() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let first = fixture.path().join("project-a");
    let second = fixture.path().join("project-b");
    std::fs::create_dir(&first).expect("应创建项目 A");
    std::fs::create_dir(&second).expect("应创建项目 B");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取工作空间").remove(0);
    for project in [&first, &second] {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                app.complete_project_picker_for_e2e(project.clone(), window, cx)
            })
        });
        cx.run_until_parked();
    }
    click_debug(cx, "desktop-workspace-back");
    click_debug(cx, "desktop-workspace-card-0");
    assert_eq!(
        app.read_with(cx, |app, _| app.desktop_loaded_area_count_for_e2e()),
        2,
        "重新打开总览应加载两张区域会话"
    );

    click_debug(cx, "desktop-workspace-back");
    std::fs::rename(&second, fixture.path().join("moved-b")).expect("应临时移走项目 B");
    click_debug(cx, "desktop-workspace-card-0");
    assert!(
        cx.debug_bounds("desktop-project-area-1").is_some(),
        "B 不可访问时区域仍应可见"
    );
    assert!(
        cx.debug_bounds("desktop-project-unavailable-1").is_some(),
        "B 不可访问时应显示明确状态"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.desktop_loaded_area_count_for_e2e()),
        2,
        "A 与已有 B 会话应保留且不因路径短时不可用被删除"
    );
    assert_eq!(
        store
            .list_projects(&workspace.id)
            .expect("应读取区域")
            .len(),
        2,
        "项目目录临时不可用不得删掉区域"
    );
}

/// DWS-03/DWS-06/DWS-07：总览卡片输入与统一指令都按项目路由，含糊指令先澄清。
///
/// 数据构造（含关键数值的推导过程）：
///   项目 A/B 各 1 个；每区 1 张 Codex Agent 卡片；直接输入 2 条、统一输入 2 条。
///   明确指令的正确路由为 A=2、B=2；含糊输入的派发数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次展开 A、B，选择项目目录并使用 echo PTY 各创建一张 Agent 卡片。
///   2. 返回总览，在 A 输入 task-A，在 B 输入 task-B 并发送。
///   3. 从统一输入分别派发给 A、B，再输入未指明项目的指令。
///   4. 停止 A 的运行实例，再次跨项目派发并读取逐目标回执。
///
/// 预期结果：
///   - 正断言：A/B 各收到自己的直接与统一指令；A 失败时 B 仍成功。
///   - 负断言：A/B 互不串线；含糊指令不送达任何 Agent；总览发送不新建窗口。
#[test]
fn desktop_overview_routes_agent_input_to_each_project_session() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("测试应创建隔离项目目录");
    let first = fixture.path().join("project-a");
    let second = fixture.path().join("project-b");
    std::fs::create_dir(&first).expect("应创建项目 A");
    std::fs::create_dir(&second).expect("应创建项目 B");
    let db = app.read_with(cx, |app, _| app.app_database_path_for_e2e());
    let store = DesktopWorkspaceStore::new(db);

    click_debug(cx, "desktop-workspace-create");
    let workspace = store.list().expect("应读取工作空间").remove(0);
    for project in [&first, &second] {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                app.complete_project_picker_for_e2e(project.clone(), window, cx)
            })
        });
        cx.run_until_parked();
    }
    let areas = store.list_projects(&workspace.id).expect("应读取两处区域");
    let mut agent_ids = Vec::new();
    for index in 0..2 {
        click_debug(
            cx,
            if index == 0 {
                "desktop-project-area-0"
            } else {
                "desktop-project-area-1"
            },
        );
        app.update(cx, |app, cx| {
            app.desktop_set_expanded_pty_factory_for_e2e(echo_factory(), cx)
        });
        cx.dispatch_action(app::NewCodexAgent);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("desktop-agent-worktree-picker").is_some(),
            "创建 Agent 前必须选择项目 worktree"
        );
        click_debug(cx, "desktop-agent-worktree-0");
        cx.run_until_parked();
        let ids = app.read_with(cx, |app, cx| app.desktop_expanded_agent_ids_for_e2e(cx));
        assert_eq!(ids.len(), 1, "每个项目区域应创建一张 Agent 卡片");
        agent_ids.push(ids[0]);
        click_debug(cx, "desktop-project-back");
    }

    click_debug(cx, "desktop-agent-input-0-0");
    cx.simulate_input("task-A");
    click_debug(cx, "desktop-agent-send-0-0");
    click_debug(cx, "desktop-agent-input-1-0");
    cx.simulate_input("task-B");
    click_debug(cx, "desktop-agent-send-1-0");
    cx.run_until_parked();
    app.update(cx, |app, cx| app.desktop_drain_area_outputs_for_e2e(cx));
    let direct_a = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[0].id, agent_ids[0], cx)
        })
        .expect("A 应有 PTY 输出");
    let direct_b = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[1].id, agent_ids[1], cx)
        })
        .expect("B 应有 PTY 输出");
    assert!(direct_a.contains("task-A"), "A 必须收到自己的直接指令");
    assert!(!direct_a.contains("task-B"), "A 不得收到 B 的直接指令");
    assert!(direct_b.contains("task-B"), "B 必须收到自己的直接指令");
    assert!(!direct_b.contains("task-A"), "B 不得收到 A 的直接指令");
    click_debug(cx, "desktop-norma-input");
    cx.simulate_input("让 project-a 修复 A; 让 project-b 跑 B");
    click_debug(cx, "desktop-norma-send");
    assert!(
        cx.debug_bounds("desktop-norma-receipt-0").is_some(),
        "A 应有独立派工回执"
    );
    assert!(
        cx.debug_bounds("desktop-norma-receipt-1").is_some(),
        "B 应有独立派工回执"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app
            .desktop_dispatch_statuses_for_e2e(&workspace.id)),
        vec!["Submitted", "Submitted"],
        "两个目标都必须提交成功而非只显示回执占位"
    );
    click_debug(cx, "desktop-norma-input");
    cx.simulate_input("修一下");
    click_debug(cx, "desktop-norma-send");
    app.update(cx, |app, cx| app.desktop_drain_area_outputs_for_e2e(cx));

    let output_a = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[0].id, agent_ids[0], cx)
        })
        .expect("A 应有 PTY 输出");
    let output_b = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[1].id, agent_ids[1], cx)
        })
        .expect("B 应有 PTY 输出");
    let compact_a: String = output_a
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let compact_b: String = output_b
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(
        compact_a.contains("修复A"),
        "统一指令应送达 A，实际输出：{compact_a}"
    );
    assert!(!compact_a.contains("跑B"), "A 不得收到 B 的统一指令");
    assert!(compact_b.contains("跑B"), "统一指令应送达 B");
    assert!(!compact_b.contains("修复A"), "B 不得收到 A 的统一指令");
    assert!(!compact_a.contains("修一下"), "含糊指令不得送达 A");
    assert!(!compact_b.contains("修一下"), "含糊指令不得送达 B");
    app.update(cx, |app, cx| {
        app.desktop_stop_area_runtime_for_e2e(&workspace.id, &areas[0].id, cx)
    });
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_running_count_for_e2e(
            &workspace.id,
            &areas[0].id,
            cx
        )),
        Some(0),
        "A 已停止后不得继续计入工作空间运行状态"
    );
    assert_eq!(
        app.read_with(cx, |app, cx| app.desktop_area_running_count_for_e2e(
            &workspace.id,
            &areas[1].id,
            cx
        )),
        Some(1),
        "B 仍应独立显示为运行中"
    );
    click_debug(cx, "desktop-norma-input");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("让 project-a 修复 A2; 让 project-b 回归 B2");
    click_debug(cx, "desktop-norma-send");
    let statuses = app.read_with(cx, |app, _| {
        app.desktop_dispatch_statuses_for_e2e(&workspace.id)
    });
    assert_eq!(statuses.len(), 2, "部分失败也应保留两个独立回执");
    assert!(
        statuses[0].starts_with("Failed:"),
        "已停止的 A 应如实显示派工失败"
    );
    assert_eq!(statuses[1], "Submitted", "A 失败不应阻止 B 成功");
    app.update(cx, |app, cx| app.desktop_drain_area_outputs_for_e2e(cx));
    let failed_a = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[0].id, agent_ids[0], cx)
        })
        .expect("A 卡片应保留历史输出");
    let succeeded_b = app
        .read_with(cx, |app, cx| {
            app.desktop_agent_grid_text_for_e2e(&workspace.id, &areas[1].id, agent_ids[1], cx)
        })
        .expect("B 应有新输出");
    assert!(!failed_a.contains("A2"), "失败的 A 不得声称收到新指令");
    assert!(succeeded_b.contains("B2"), "A 失败时 B 仍应收到自己的任务");
    assert_eq!(cx.cx.windows().len(), 1, "总览直接操作不应产生新窗口");
}
