use crate::app::MultisoulApp;
use crate::norma::endpoint::presentation::dispatch;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use norma::protocol::{
    AgentProfile, PresentationDisposition, PresentationLayout, ProviderAuthorization,
};
use norma::protocol::{CardCommand, CardCommandResult, PresentationTarget, WireRequest};

fn with_desktop(
    test: impl FnOnce(Entity<MultisoulApp>, Entity<MultisoulApp>, WireRequest, &mut VisualTestContext),
) {
    let temp = tempfile::tempdir().expect("隔离项目");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Presentation")
        .expect("工作空间");
    let area = store
        .desktop_workspace_store()
        .add_project(&workspace.id, &root)
        .expect("项目区");
    let auth = storage::OrchestrationAuthorizationStore::new(paths.app_db_path().to_owned());
    auth.authorize("codex", "fixture-token", "1")
        .expect("隔离授权");
    let cache = storage::OrchestrationAuthorizationCache::new(auth.list().expect("授权缓存"));
    let mut context = TestAppContext::single();
    let mut host = None;
    let (_, cx) = context.add_window_view(|window, cx| {
        crate::norma::endpoint::install_at_path(temp.path().join("endpoint.sock"), cx)
            .expect("隔离 endpoint");
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("宿主");
    host.update_in(cx, |app, window, cx| {
        app.browsers.disable_native_views();
        app.open_desktop_workspace(workspace.id.clone(), window, cx)
    });
    cx.run_until_parked();
    let project = host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.expanded_desktop_project().expect("展开项目")
    });
    let request = project.update_in(cx, |app, window, _| {
        app.browsers.disable_native_views();
        app.orchestration_authorizations = cache;
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
        let result = crate::app::execute_agent_card_command(
            app,
            window,
            CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: AgentProfile::Codex,
                activate: true,
            },
        )
        .expect("创建来源 Agent");
        let CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        } = result
        else {
            panic!("来源必须激活");
        };
        WireRequest {
            protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
            client_version: None,
            request_id: 1,
            operation_id: None,
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "fixture-token".into(),
            }),
            source_workspace_id: app.workspace_id.clone(),
            source_workspace_route_id: app.workspace_route_id.clone(),
            source_node_id: Some(node_id),
            target_workspace_id: None,
            command: CardCommand::Present {
                target: PresentationTarget::Browser {
                    url: "https://example.com/".into(),
                },
                create_new: false,
            },
        }
    });
    test(host, project, request, cx);
}

/// 场景：连续 Present 必须逐条完成抽屉安装与回执，且不能创建隐藏画布卡片。
/// 数据构造：真实桌面宿主、项目来源 Agent、有效授权、两个同址网页请求。
/// 执行过程：通过生产 endpoint 连续分发，第二次使用 --new，等待后台队列完成。
/// 预期结果：每个请求都有 Created/ContentDrawer 回执、不同真实 ID；画布节点及 Master 不变。
#[test]
fn desktop_presentation_endpoint_serializes_receipts_without_canvas_mutation() {
    with_desktop(|host, project, mut request, cx| {
        let before = project.read_with(cx, |app, _| app.runtime.document.nodes().to_vec());
        let (send_a, receive_a) = crossbeam_channel::bounded(1);
        let (send_b, receive_b) = crossbeam_channel::bounded(1);
        cx.update(|_, cx| dispatch(request.clone(), send_a, cx));
        request.request_id = 2;
        if let CardCommand::Present { create_new, .. } = &mut request.command {
            *create_new = true;
        }
        cx.update(|_, cx| dispatch(request, send_b, cx));
        cx.run_until_parked();
        let mut ids = Vec::new();
        for receive in [receive_a, receive_b] {
            let result = receive
                .try_recv()
                .expect("每条请求必须收到回执")
                .result
                .expect("展示成功");
            let CardCommandResult::Presented {
                node_id,
                layout,
                disposition,
                ..
            } = result
            else {
                panic!("必须返回 Presented");
            };
            assert_eq!(
                layout,
                PresentationLayout::ContentDrawer,
                "回执必须反映右抽屉"
            );
            assert_eq!(
                disposition,
                PresentationDisposition::Created,
                "首次与 --new 均创建独立标签"
            );
            ids.push(node_id);
        }
        assert_ne!(ids[0], ids[1], "--new 标签必须有独立表面 ID");
        host.read_with(cx, |app, _| {
            assert_eq!(
                app.desktop_workspace.content_tabs.len(),
                2,
                "两条展示均应持久化并安装"
            );
            assert!(
                app.desktop_workspace.drawers.right_open,
                "成功回执前必须打开内容抽屉"
            );
        });
        project.read_with(cx, |app, _| {
            assert_eq!(
                app.runtime.document.nodes(),
                before,
                "展示内容不得新增隐藏节点或改变画布边界"
            );
            assert!(
                !app.master_layout.is_active(),
                "内容抽屉不得偷偷进入 Master"
            );
        });
    });
}

/// 场景：来源在异步展示期间失效，不能把排队成功当成展示成功。
/// 数据构造：有效来源与网页请求；分别模拟删除来源卡片、切换桌面 epoch、撤销授权。
/// 执行过程：分发后、后台任务回填前使来源失效，再运行队列。
/// 预期结果：每条请求都有失败回执，当前桌面没有被打开的内容标签。
#[test]
fn desktop_presentation_endpoint_rechecks_source_before_showing() {
    for invalidation in 0..3 {
        with_desktop(|host, project, request, cx| {
            let (send, receive) = crossbeam_channel::bounded(1);
            let source = request.source_node_id.expect("来源节点");
            cx.update(|_, cx| dispatch(request, send, cx));
            match invalidation {
                0 => project.update(cx, |app, _| {
                    app.runtime.remove_node(source);
                }),
                1 => host.update(cx, |app, _| {
                    app.desktop_workspace.active_id = None;
                    app.desktop_workspace.content_workspace_epoch += 1;
                }),
                _ => project.update(cx, |app, _| {
                    app.orchestration_authorizations = Default::default();
                }),
            }
            cx.run_until_parked();
            assert!(
                receive
                    .try_recv()
                    .expect("失效请求也必须有回执")
                    .result
                    .is_err(),
                "来源失效不得报告 Presented"
            );
            host.read_with(cx, |app, _| {
                assert!(
                    app.desktop_workspace.content_tabs.is_empty(),
                    "失效来源不能在当前桌面展示内容"
                )
            });
        });
    }
}

/// 场景：Present 与紧随其后的用户关闭操作必须按发起顺序执行。
/// 数据构造：真实桌面与已存在的同址网页 X，不为预处理提供测试 hook。
/// 执行过程：同一前台 turn 分发 Present(X)，立即关闭 X，再等待后台队列。
/// 预期结果：Present 复用原 ID 并回执；最终 UI 与存储均无 X，不能被迟到预处理复活。
#[test]
fn desktop_presentation_endpoint_preserves_order_with_user_close() {
    with_desktop(|host, _, request, cx| {
        host.update(cx, |app, cx| {
            app.open_desktop_content_tab(
                storage::DesktopContentKind::Browser,
                "https://example.com/".into(),
                "X".into(),
                None,
                cx,
            );
        });
        cx.run_until_parked();
        let tab = host.read_with(cx, |app, _| app.desktop_workspace.content_tabs[0].clone());
        let (send, receive) = crossbeam_channel::bounded(1);
        cx.update(|_, cx| {
            dispatch(request, send, cx);
            host.update(cx, |app, cx| app.close_desktop_content_tab(&tab.id, cx));
        });
        cx.run_until_parked();
        let result = receive
            .try_recv()
            .expect("Present 必须完成回执")
            .result
            .expect("Present 成功");
        let CardCommandResult::Presented {
            node_id,
            disposition,
            ..
        } = result
        else {
            panic!("必须返回 Presented");
        };
        assert_eq!(
            disposition,
            PresentationDisposition::Reused,
            "先发 Present 必须复用已有 X"
        );
        assert_eq!(
            node_id,
            super::super::content_node_id(&tab.id),
            "必须是原标签表面"
        );
        host.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.content_tabs.is_empty(),
                "后发关闭后标签不能复活"
            );
            let saved =
                storage::DesktopContentTabStore::new(app.database_paths.app_db_path().to_owned())
                    .list(&tab.workspace_id)
                    .expect("读取存储");
            assert!(saved.is_empty(), "关闭也必须持久化，重启不能复活 X");
        });
    });
}
