//! 跨 Workspace 任务结果：来源 Agent 必须用全局 task_ref 读取目标 Agent 的可信回复。

use app::MultisoulApp;
use gpui::{AppContext as _, Entity, TestAppContext, WindowHandle, WindowOptions};
use norma::protocol::{
    AgentProfile, AgentTaskReceiptStatus, AgentTaskResultAvailability, CardCommand,
    CardCommandResult, ProviderAuthorization, WireRequest, WireResponse, CURRENT_PROTOCOL_VERSION,
};
use ui::component::Root;

type TestWindow = WindowHandle<Root>;

/// E2E-CROSS-RESULT-01：跨 Workspace status/wait 与 result 必须返回同一全局任务结果。
///
/// 数据构造（含关键数值的推导过程）：
///   Workspace windows = 来源 A(1) + 目标 B(1) = 2
///   agents = Alan/A(1) + Bella/B(1) = 2
///   dispatch tasks = 1，global task_ref 取跨 Workspace assign 回执
///   提交等待预算 = 120ms > 输入后 Enter 延迟 100ms
///   Stop.final_response = "Workspace B 检查完成"，字符数 16 < 1200 → truncated=false
///   runtime frame = 16ms，每次 Hook 后至少推进 1 帧
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在同一 App 中打开 A/B 两个 Workspace，并分别激活 Alan/Bella。
///   2. 为 B/Bella 注入 SessionStart，再从 A 向 B 派发唯一任务。
///   3. 注入 UserPromptSubmit 与携带最终回复的可信 Stop，使 B 的本地任务进入 Done。
///   4. 从 A 通过全局 task_ref 分别查询 AgentTaskStatus 与 AgentTaskResult。
///
/// 预期结果：
///   - 正断言：两条查询均返回 Done、available、未截断的同一精确回复。
///   - 正断言：返回 task_id 保持全局引用，不泄漏 B 的本地 runtime task ref。
///   - 负断言：结果不得进入 A coordinator、归属 Alan 或退化为 unavailable。
#[test]
fn cross_workspace_status_and_result_return_the_same_global_task_reply() {
    let fixture = tempfile::tempdir().expect("跨 Workspace 结果 e2e 应创建隔离根目录");
    let workspace_a = fixture.path().join("workspace-a");
    let workspace_b = fixture.path().join("workspace-b");
    std::fs::create_dir_all(&workspace_a).expect("测试应创建 Workspace A");
    std::fs::create_dir_all(&workspace_b).expect("测试应创建 Workspace B");
    let database_paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let socket_path = fixture.path().join("norma-result-e2e.sock");
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
            .expect("测试应打开来源 Workspace A 窗口");
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
            .expect("测试应打开目标 Workspace B 窗口");
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
            "cross-result-token",
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

    enqueue_hook(
        &app_b,
        &mut test_cx,
        norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            target_node_id,
            serde_json::json!({
                "hook_event_name": "SessionStart",
                "session_id": "cross-result-session"
            }),
        )
        .expect("B SessionStart 应可解析"),
    );
    let assigned = submit(
        request(
            201,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            Some(workspace_b_id),
            Some("22222222-3333-4444-8555-666666666666".to_string()),
            CardCommand::DispatchAgentTasks {
                assignments: vec![norma::protocol::AgentTaskAssignment {
                    name: "Bella".to_string(),
                    task: "检查 Workspace B".to_string(),
                }],
                force_untracked: false,
            },
        ),
        &mut test_cx,
    );
    let Ok(CardCommandResult::AgentTasksDispatched { receipts }) = assigned.result else {
        panic!("跨 Workspace assign 必须返回任务回执");
    };
    assert_eq!(receipts.len(), 1, "单目标跨 Workspace 派发必须只有一条回执");
    let global_task_ref = receipts[0].task_id.clone();

    std::thread::sleep(std::time::Duration::from_millis(120));
    advance_runtime_frame(&mut test_cx);
    enqueue_hook(
        &app_b,
        &mut test_cx,
        norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            target_node_id,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit",
                "session_id": "cross-result-session",
                "prompt": "检查 Workspace B"
            }),
        )
        .expect("B UserPromptSubmit 应可解析"),
    );
    enqueue_hook(
        &app_b,
        &mut test_cx,
        norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            target_node_id,
            serde_json::json!({
                "hook_event_name": "Stop",
                "session_id": "cross-result-session",
                "final_response": "Workspace B 检查完成"
            }),
        )
        .expect("B Stop 应可解析"),
    );

    let status = submit(
        request(
            202,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            None,
            None,
            CardCommand::AgentTaskStatus {
                task_ref: global_task_ref.clone(),
            },
        ),
        &mut test_cx,
    );
    let result = submit(
        request(
            203,
            &workspace_a_id,
            &route_a_id,
            source_node_id,
            None,
            None,
            CardCommand::AgentTaskResult {
                task_ref: global_task_ref.clone(),
            },
        ),
        &mut test_cx,
    );
    let Ok(CardCommandResult::AgentTaskStatus { task: status_task }) = status.result else {
        panic!("跨 Workspace status 必须返回全局任务状态");
    };
    let Ok(CardCommandResult::AgentTaskResult { task: result_task }) = result.result else {
        panic!("跨 Workspace result 必须返回全局任务结果");
    };
    let source_task_count = test_cx.update(|cx| app_a.read(cx).agent_task_count_for_e2e());

    assert_eq!(
        status_task.status,
        AgentTaskReceiptStatus::Done,
        "B 的可信 Stop 必须通过全局 status 对账为 Done"
    );
    let bound_result = status_task
        .result
        .as_ref()
        .expect("跨 Workspace Done 状态必须直接携带结果");
    assert_eq!(
        bound_result.availability,
        AgentTaskResultAvailability::Available,
        "B 提供最终回复时全局结果必须 available"
    );
    assert_eq!(
        bound_result.text.as_deref(),
        Some("Workspace B 检查完成"),
        "A 必须读取 B 当前任务的精确最终回复"
    );
    assert!(!bound_result.truncated, "短跨 Workspace 回复不得标记为截断");
    assert_eq!(
        result_task, status_task,
        "独立 result 与 wait/status 必须读取同一全局任务结果"
    );
    assert_eq!(
        result_task.task_id, global_task_ref,
        "返回结果必须保留全局 task_ref，不得泄漏 B 的本地 runtime ref"
    );
    assert_eq!(
        source_task_count, 0,
        "B 的任务与结果不得错误进入来源 A coordinator"
    );
    assert_ne!(
        result_task.target_name, "Alan",
        "跨 Workspace 结果不得错误归属给来源 A/Alan"
    );
    assert_ne!(
        result_task
            .result
            .expect("result 查询必须返回绑定结果")
            .availability,
        AgentTaskResultAvailability::Unavailable,
        "存在可信 final_response 时全局 result 不得退化为 unavailable"
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
            token: "cross-result-token".to_string(),
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

fn enqueue_hook(
    app: &Entity<MultisoulApp>,
    test_cx: &mut TestAppContext,
    event: norma::agent_hook::AgentHookEvent,
) {
    app.update(test_cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(event)
            .expect("跨 Workspace Hook 应进入全局 ingress")
    });
    advance_runtime_frame(test_cx);
}

fn advance_runtime_frame(test_cx: &mut TestAppContext) {
    test_cx
        .background_executor
        .advance_clock(std::time::Duration::from_millis(16));
    test_cx.run_until_parked();
}
