use super::*;
use crate::app::background_project::PreparedProject;
use agent::{context::*, AdapterCommand};
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};

/// 场景：模型把待办想法派给另一个项目的 Agent，来源项目用户已说“只修改前端”。
/// 构造：桌面空间含 A（来源）与 B（目标，Recording PTY）；A 保存 communicated 约束并记一条想法。
/// 执行：经生产守卫入口 execute_norma_adapter_command_with_source 执行 AssignIdea。
/// 预期：B 的卡片实际收到想法原文与“只修改前端”约束，想法变为已指派。
#[test]
fn assign_idea_carries_source_constraint_to_target_card() {
    let temp = tempfile::tempdir().unwrap();
    let (root_a, root_b) = (temp.path().join("A"), temp.path().join("B"));
    std::fs::create_dir(&root_a).unwrap();
    std::fs::create_dir(&root_b).unwrap();
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).unwrap();
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Ideas").unwrap();
    let area_a = catalog.add_project(&workspace.id, &root_a).unwrap();
    let area_b = catalog.add_project(&workspace.id, &root_b).unwrap();
    let prepared_a = PreparedProject::load_for_desktop(paths.clone(), area_a.clone()).unwrap();
    let prepared_b = PreparedProject::load_for_desktop(paths.clone(), area_b.clone()).unwrap();
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let writes_b = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let desktop_host = host.downgrade();
    let make_session = |prepared: PreparedProject,
                        writes: Arc<Mutex<Vec<Vec<u8>>>>,
                        cx: &mut gpui::VisualTestContext| {
        cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.desktop_host = Some(desktop_host.clone());
            app.runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
                Ok(Box::new(terminal::TestPtyBackend::recording(
                    tx,
                    writes.clone(),
                )))
            }));
            app
        })
    };
    let session_a = make_session(prepared_a, Arc::new(Mutex::new(Vec::new())), cx);
    let session_b = make_session(prepared_b, writes_b.clone(), cx);
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone(), area_b.clone()];
        app.desktop_workspace.expanded_area_id = Some(area_a.id.clone());
        for (area, session) in [(&area_a, &session_a), (&area_b, &session_b)] {
            app.desktop_workspace
                .sessions
                .insert((workspace.id.clone(), area.id.clone()), session.clone());
        }
    });
    session_b.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();
    let node_b = session_b.read_with(cx, |app, _| {
        app.runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)))
            .unwrap()
            .id
    });
    writes_b.lock().unwrap().clear();
    let service = session_a.update(cx, |app, _| app.shared_context_service().unwrap());
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("limit".into()), |access| {
            let input = access.record_input("limit", ContextOrigin::Voice, "只修改前端")?;
            access.update(
                "limit",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "frontend".into(),
                        text: "只修改前端".into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Communicated,
                    },
                },
            )
        }))
        .unwrap();
    // Production prepares both gates on the background executor before the sync dispatch.
    let target_service = session_b.update(cx, |app, _| app.shared_context_service().unwrap());
    for service in [&service, &target_service] {
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
    }
    let captured = session_a
        .update(cx, |app, cx| {
            capture_idea(app, cx, "检查登录方案", AgentInputSource::Other)
        })
        .unwrap();
    let idea_id = captured.tasks[0]["idea_id"].as_str().unwrap().to_owned();
    let accepted = session_a.update(cx, |app, cx| {
        crate::app::execute_norma_adapter_command_with_source(
            app,
            None,
            cx,
            AdapterCommand::AssignIdea {
                idea_id: idea_id.clone(),
                target: IdeaTarget::Agent {
                    workspace_id: workspace.id.clone(),
                    area_id: area_b.id.clone(),
                    node_id: node_b,
                },
            },
            AgentInputSource::Voice,
        )
    });
    assert!(accepted.is_ok(), "想法派发应被接受：{accepted:?}");
    // The task is held until the idea's meteor lands (about 0.9s), exactly like the existing flow.
    std::thread::sleep(std::time::Duration::from_millis(950));
    session_b.update(cx, |app, cx| {
        app.submit_due_pending_agent_tasks(std::time::Instant::now(), cx);
    });
    let sent = String::from_utf8_lossy(&writes_b.lock().unwrap().concat()).into_owned();
    assert!(
        sent.contains("检查登录方案") && sent.contains("只修改前端"),
        "想法任务必须携带来源项目的当前约束：{sent}"
    );
}
