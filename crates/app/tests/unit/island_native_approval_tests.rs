use crate::app::{AppScreen, MultisoulApp};
use gpui::{AppContext, TestAppContext};
use norma::agent_hook::{parse_hook_event, HookSource};

/// 场景：原生审批菜单尚未出现或窄卡片截断菜单时，岛仍须提醒用户定位到原卡。
/// 数据：隔离工作空间、Claude 原生会话及准确 PermissionRequest；PTY 不输出可验证菜单。
/// 执行：通过运行态 Hook 与工作空间活动刷新入口发布全局投影。
/// 预期：保留原卡审批身份和待处理计数，但没有允许/拒绝权限，不能漏报或猜测放行。
#[test]
fn island_activity_keeps_native_approval_without_actionable_menu() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Native approval").expect("创建工作空间");
    let area = catalog
        .add_project(&workspace.id, temp.path())
        .expect("添加项目");
    let transcript = temp.path().join("session-a.jsonl");
    std::fs::write(&transcript, "").expect("创建隔离会话记录");
    let mut cx = TestAppContext::single();
    let host =
        cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        );
        app.desktop_host = Some(host.downgrade());
        app.enter_canvas_for_test(temp.path(), cx);
        let node = app
            .runtime
            .spawn_agent(
                canvas::AgentProfile::Claude,
                canvas::CanvasRect::new(0., 0., 400., 400.),
                46,
                30,
            )
            .expect("启动原卡");
        let run = app.runtime.mobile_agent_run_id(node).map(str::to_owned);
        for name in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
        ] {
            let mut event = parse_hook_event(
                HookSource::Claude,
                node,
                serde_json::json!({
                    "hook_event_name":name,"session_id":"session-a","prompt_id":"turn-a",
                    "transcript_path":transcript,"tool_name":"Bash","tool_use_id":"tool-a",
                    "tool_input":{"command":"printf approval","description":"Write marker"}
                }),
            )
            .expect("解析生产Hook");
            event.control_run_id = run.clone();
            assert!(
                app.runtime.observe_mobile_hook(&event),
                "准确 {name} 应被接受"
            );
        }
        app.runtime
            .apply_agent_task_update(runtime::AgentTaskUpdate {
                node_id: node,
                profile: canvas::AgentProfile::Claude,
                provider_event: "PermissionRequest".into(),
                to: runtime::AgentTaskState::WaitingApproval {
                    tool_name: Some("Bash".into()),
                },
                task_title: None,
                session_id: Some("session-a".into()),
            });
        app
    });
    let model = cx.update(crate::island::model);
    host.update(&mut cx, |app, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = catalog.list().expect("读取目录");
        app.desktop_workspace.areas = catalog.list_projects(&workspace.id).expect("读取项目");
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session);
        app.note_workspace_activity_changed(cx);
    });
    cx.run_until_parked();
    model.read_with(&cx, |model, _| {
        let project = &model.workspaces[0].projects[0];
        assert_eq!(
            project.approvals.len(),
            1,
            "无法就地操作的审批仍须保留在岛上"
        );
        let approval = &project.approvals[0];
        assert_eq!(approval.target.session_id, "session-a", "提醒绑定原会话");
        assert_eq!(approval.target.turn_id, "turn-a", "提醒绑定原回合");
        assert!(approval.decisions.is_empty(), "未经验证的原生菜单不能放行");
        assert_eq!(
            approval.tool, "native_terminal",
            "保留原生处理提示而非伪造工具按钮"
        );
    });
}
