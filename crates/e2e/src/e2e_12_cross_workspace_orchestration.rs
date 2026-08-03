use app::MultisoulApp;
use gpui::{AppContext as _, Entity, TestAppContext, WindowHandle, WindowOptions};
use norma::protocol::{
    AgentProfile, CardCommand, CardCommandResult, ProviderAuthorization, WireRequest, WireResponse,
    CURRENT_PROTOCOL_VERSION,
};
use ui::component::Root;

type TestWindow = WindowHandle<Root>;

/// 双 Workspace 编排闭环：来源 A 必须经全局路由把唯一任务送入后台 B。
///
/// 数据构造（含关键数值的推导过程）：
///   App-global endpoints = Norma(1) + Hook ingress(1)
///   Workspace windows = A(1) + B(1) = 2 个
///   colliding node IDs = Alan/A node 1 + Bella/B node 1（两个文档各自从 1 开始）
///   commands = workspace list(1) + target agent list(1) + assign(1) = 3 条
///   expected target tasks = B(1)，expected source tasks = A(0)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在同一 GPUI App 中安装一个隔离 Norma endpoint，并打开 A、B 两个窗口。
///   2. 两个窗口分别注册稳定 Workspace/route，A 激活 Alan，B 激活 Bella。
///   3. 以 A 的授权和 route 调用 workspace list，再显式查询 B 的 Agent。
///   4. 使用固定 operation ID 向 B/Bella 派发任务，推进全局账本与目标 coordinator。
///   5. 比较回执、两个 coordinator、目标 cwd 和活动窗口。
///
/// 预期结果：
///   - 正断言：全局名册只含两个注册 Workspace，目标名册只返回 B/Bella。
///   - 正断言：任务回执携带 A→B 身份，且只进入 B 的 coordinator 和 B cwd。
///   - 正断言：B 在后台仍接收任务，活动窗口继续是 A。
///   - 负断言：相同 node ID 不得使任务进入 A，也不得把 Alan 当作 B 的目标。
#[test]
fn cross_workspace_assign_routes_to_background_target_without_node_id_collision() {
    let fixture = tempfile::tempdir().expect("跨 Workspace e2e 应创建隔离根目录");
    let workspace_a = fixture.path().join("workspace-a");
    let workspace_b = fixture.path().join("workspace-b");
    std::fs::create_dir_all(&workspace_a).expect("测试应创建 Workspace A");
    std::fs::create_dir_all(&workspace_b).expect("测试应创建 Workspace B");
    let database_paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let socket_path = fixture.path().join("norma-e2e.sock");
    let mut test_cx = TestAppContext::single();

    let (window_a, app_a, window_b, app_b) = test_cx.update(|cx| {
        ui::init_components(cx);
        app::install_norma_endpoint_for_e2e(socket_path, cx)
            .expect("测试 App-global Norma endpoint 应成功安装一次");
        let mut app_a = None;
        let window_a = cx
            .open_window(WindowOptions::default(), |window, cx| {
                let app = cx.new(|cx| {
                    MultisoulApp::new_with_pty_factory_and_database_paths(
                        cx,
                        crate::harness::echo_factory(),
                        database_paths.clone(),
                    )
                });
                app_a = Some(app.clone());
                cx.new(|cx| Root::new(app, window, cx))
            })
            .expect("测试应打开 Workspace A 窗口");
        let mut app_b = None;
        let window_b = cx
            .open_window(WindowOptions::default(), |window, cx| {
                let app = cx.new(|cx| {
                    MultisoulApp::new_with_pty_factory_and_database_paths(
                        cx,
                        crate::harness::echo_factory(),
                        database_paths,
                    )
                });
                app_b = Some(app.clone());
                cx.new(|cx| Root::new(app, window, cx))
            })
            .expect("测试应打开 Workspace B 窗口");
        (
            window_a,
            app_a.expect("A 窗口必须创建 MultisoulApp"),
            window_b,
            app_b.expect("B 窗口必须创建 MultisoulApp"),
        )
    });

    let source_node_id =
        prepare_workspace_agent(&window_a, &app_a, &mut test_cx, &workspace_a, "Alan");
    let target_node_id =
        prepare_workspace_agent(&window_b, &app_b, &mut test_cx, &workspace_b, "Bella");
    window_a
        .update(&mut test_cx, |_root, window, _cx| window.activate_window())
        .expect("测试应把来源 Workspace A 设为活动窗口");
    test_cx.run_until_parked();
    test_cx.update(app::mark_orchestration_health_ready_for_e2e);
    app_a.update(&mut test_cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            fixture.path().join("authorization.sqlite"),
            "codex",
            "cross-workspace-token",
        )
        .expect("来源 Workspace A 应安装隔离 Codex 授权");
    });
    let (workspace_a_id, route_a_id) = test_cx.update(|cx| {
        app_a
            .read(cx)
            .norma_workspace_identity_for_e2e()
            .expect("Workspace A 必须已注册 route")
    });
    let (workspace_b_id, _) = test_cx.update(|cx| {
        app_b
            .read(cx)
            .norma_workspace_identity_for_e2e()
            .expect("Workspace B 必须已注册 route")
    });
    let active_before = test_cx.update(|cx| cx.active_window());

    let workspaces = submit(
        request(
            101,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            None,
            None,
            CardCommand::ListWorkspaces,
        ),
        &mut test_cx,
    );
    let target_agents = submit(
        request(
            102,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            Some(workspace_b_id.clone()),
            None,
            CardCommand::ListAgents,
        ),
        &mut test_cx,
    );
    let assigned = submit(
        request(
            103,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            Some(workspace_b_id.clone()),
            Some("11111111-2222-4333-8444-555555555555".to_string()),
            CardCommand::DispatchAgentTasks {
                assignments: vec![norma::protocol::AgentTaskAssignment {
                    name: "Bella".to_string(),
                    task: "只在 Workspace B 运行测试".to_string(),
                }],
                force_untracked: false,
            },
        ),
        &mut test_cx,
    );
    let active_after = test_cx.update(|cx| cx.active_window());

    let Ok(CardCommandResult::WorkspacesListed { workspaces }) = workspaces.result else {
        panic!("workspace list 必须返回全局已注册 Workspace 名册");
    };
    let Ok(CardCommandResult::AgentsListed { agents }) = target_agents.result else {
        panic!("显式 B agent list 必须返回目标 Workspace 名册");
    };
    let Ok(CardCommandResult::AgentTasksDispatched { receipts }) = assigned.result else {
        panic!("跨 Workspace assign 必须返回任务回执");
    };
    let receipt = receipts.first().expect("单目标派发必须返回一条回执");
    let (source_task_count, target_task_count, target_cwd) = test_cx.update(|cx| {
        (
            app_a.read(cx).agent_task_count_for_e2e(),
            app_b.read(cx).agent_task_count_for_e2e(),
            app_b
                .read(cx)
                .runtime
                .workspace_root()
                .map(ToOwned::to_owned),
        )
    });

    assert_eq!(
        workspaces.len(),
        2,
        "workspace list 必须只返回 A、B 两个已注册 Workspace"
    );
    assert_eq!(agents.len(), 1, "显式查询 B 时只能返回 B 的一个 Agent");
    assert_eq!(agents[0].name, "Bella", "目标名册必须精确返回 B/Bella");
    assert!(
        agents.iter().all(|agent| agent.name != "Alan"),
        "目标 B 名册不得因 node ID 碰撞混入 A/Alan"
    );
    assert_eq!(
        source_node_id, target_node_id,
        "本用例必须真实制造两个 Workspace 的相同 node ID"
    );
    assert_eq!(
        receipt.source_workspace_id.as_deref(),
        Some(workspace_a_id.as_str()),
        "回执必须记录真实来源 Workspace A"
    );
    assert_eq!(
        receipt.target_workspace_id.as_deref(),
        Some(workspace_b_id.as_str()),
        "回执必须记录显式目标 Workspace B"
    );
    assert!(
        receipt.task_id.starts_with("task-"),
        "跨 Workspace 回执必须暴露全局 task ref，不能泄漏 B 的本地 runtime ref"
    );
    assert_eq!(
        source_task_count, 0,
        "跨 Workspace 任务不得进入 A coordinator"
    );
    assert_eq!(
        target_task_count, 1,
        "跨 Workspace 任务必须且只能进入 B coordinator"
    );
    assert_eq!(
        target_cwd.as_deref(),
        Some(workspace_b.as_path()),
        "目标 Agent 必须在 Workspace B cwd 中执行"
    );
    assert!(
        active_after == active_before,
        "后台 B 接收任务时不得抢占来源 A 的活动窗口"
    );
}

fn prepare_workspace_agent(
    window_handle: &TestWindow,
    app: &Entity<MultisoulApp>,
    test_cx: &mut TestAppContext,
    workspace: &std::path::Path,
    name: &str,
) -> u64 {
    window_handle
        .update(test_cx, |_root, window, cx| {
            app.update(cx, |app, cx| {
                app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx);
                let result = app
                    .execute_agent_card_command_for_e2e(
                        window,
                        CardCommand::CreateAgent {
                            name: Some(name.to_string()),
                            profile: AgentProfile::Codex,
                            activate: true,
                        },
                    )
                    .expect("测试 Agent 创建与激活应成功");
                let CardCommandResult::AgentCreated {
                    node_id: Some(node_id),
                    ..
                } = result
                else {
                    panic!("测试 Agent 激活必须返回 node_id");
                };
                node_id
            })
        })
        .expect("测试 Workspace 窗口应保持可更新")
}

fn request(
    request_id: u64,
    source_workspace_id: &str,
    source_workspace_route_id: &str,
    source_node_id: u64,
    target_workspace_id: Option<String>,
    operation_id: Option<String>,
    command: CardCommand,
) -> WireRequest {
    WireRequest {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        request_id,
        operation_id,
        authorization: Some(ProviderAuthorization {
            provider: AgentProfile::Codex,
            token: "cross-workspace-token".to_string(),
        }),
        source_workspace_id: Some(source_workspace_id.to_string()),
        source_workspace_route_id: Some(source_workspace_route_id.to_string()),
        target_workspace_id,
        source_node_id: Some(source_node_id),
        command,
    }
}

fn submit(request: WireRequest, test_cx: &mut TestAppContext) -> WireResponse {
    let receiver = test_cx.update(|cx| app::submit_norma_request_for_e2e(request, cx));
    test_cx.run_until_parked();
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("全局编排请求应在 2 秒测试预算内返回")
}
