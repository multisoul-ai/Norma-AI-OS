use super::*;
use crate::app::background_project::PreparedProject;
use crate::app::AppScreen;
use gpui::{AppContext as _, TestAppContext};

/// Norma 定位工具必须先校验卡片与待审请求，再改变中央项目画布。
///
/// 数据构造：工作空间含项目 B，已加载一张 Codex 卡片，当前仍在空间总览。
/// 执行过程：用过期审批回合尝试定位，再用同一张卡片的稳定 ID 普通定位。
/// 预期结果：过期审批拒绝且不切项目；普通定位展开 B 并选中原卡片。
#[test]
fn workspace_agent_locator_rejects_stale_approval_then_focuses_exact_card() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("ProjectB");
    std::fs::create_dir(&root).expect("应创建 B 项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Dispatch").expect("应创建桌面空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加 B 项目");
    let prepared =
        PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("应准备 B 项目实例");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let desktop_host = host.downgrade();
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(desktop_host);
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
        app
    });
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    session.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    let node_id = session.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .expect("B 应有 Agent 卡片")
            .id
    });
    let stale = host.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(
            AgentLocation {
                workspace_id: &workspace.id,
                area_id: &area.id,
                node_id,
                approval_run_id: Some("old-run"),
                approval_turn_id: Some("old-turn"),
            },
            window,
            cx,
        )
    });
    assert!(stale.is_err(), "非当前审批请求必须拒绝定位");
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "拒绝过期审批不得切换中央画布"
        );
    });
    let unavailable = host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.area_errors.insert(
            area.id.clone(),
            ui::i18n::LocalizedText::new("workspace.project_directory_is_still_unavailable"),
        );
        app.norma_locate_workspace_agent(agent_at(&workspace.id, &area.id, node_id), window, cx)
    });
    assert!(unavailable.is_err(), "目录不可用时定位不得谎报成功");
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "失败后仍应停在总览"
        );
        app.desktop_workspace.area_errors.remove(&area.id);
    });
    let accepted = host.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(agent_at(&workspace.id, &area.id, node_id), window, cx)
            .expect("有效 Agent 卡片应可定位")
    });
    assert_eq!(accepted.node_id, Some(node_id), "定位回执应指向原卡片");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area.id.as_str()),
            "中央应展开 B 项目"
        );
    });
    session.read_with(cx, |app, _| {
        assert!(app.runtime.selection.contains(node_id), "原卡片应选中");
    });
    session.update(cx, |app, cx| {
        app.runtime.selection.clear();
        cx.notify();
    });
    session.read_with(cx, |app, _| {
        assert!(
            !app.runtime.selection.contains(node_id),
            "新定位前先清除旧选择"
        );
    });
    // 场景：Norma 从项目会话定位本项目的 Agent。
    // 数据构造：当前展开项目就是命令来源，目标仍是同一张 Codex 卡片。
    // 执行过程：在项目实体的 update 回调内调用真实定位工具，再执行延后导航。
    // 预期结果：工具不重入读取或更新项目；最终仍选中目标卡片。
    let from_project = session.update_in(cx, |app, window, cx| {
        app.norma_locate_workspace_agent(agent_at(&workspace.id, &area.id, node_id), window, cx)
            .expect("项目会话定位自身 Agent 不应重入")
    });
    assert_eq!(
        from_project.node_id,
        Some(node_id),
        "项目会话定位回执应指向原卡片"
    );
    cx.run_until_parked();
    session.read_with(cx, |app, _| {
        assert!(
            app.runtime.selection.contains(node_id),
            "延后导航后原卡片应选中"
        );
    });
    session.update(cx, |app, _| app.runtime.kill_all());
}

/// 不带审批的 Agent 定位目标。
pub(super) fn agent_at<'a>(
    workspace_id: &'a str,
    area_id: &'a str,
    node_id: canvas::NodeId,
) -> AgentLocation<'a> {
    AgentLocation {
        workspace_id,
        area_id,
        node_id,
        approval_run_id: None,
        approval_turn_id: None,
    }
}
