//! 卡片自我派发拒绝的端到端验收：SelfDispatchForbidden 带身份、换 operation_id 仍被拒、零副作用。

use app::MultisoulApp;
use gpui::{AppContext as _, Entity, TestAppContext, WindowHandle, WindowOptions};
use norma::protocol::{
    AgentErrorCode, AgentProfile, AgentTaskAssignment, CardCommand, CardCommandError,
    CardCommandResult, ProviderAuthorization, WireRequest, WireResponse, CURRENT_PROTOCOL_VERSION,
};
use ui::component::Root;

type TestWindow = WindowHandle<Root>;

/// SD-E2E-1：浩宇卡片通过 CLI 把任务派给自己，必须返回带身份的 self_dispatch_forbidden 且零副作用。
///
/// 数据构造（含关键数值的推导过程）：
///   Workspace windows = 1
///   Agent cards       = 浩宇/Codex 1 张（source 与 target 为同一节点）
///   trusted root      = 1（浩宇当前用户根任务）
///   wire requests     = 2（同目标、两个不同 operation_id）
///   expected errors   = 2 次 SelfDispatchForbidden（retryable=false）
///   expected new tasks= 0（自派发不得新增 coordinator 任务）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装隔离 Norma endpoint，创建并激活浩宇，注册可信根任务。
///   2. 提交带浩宇 source_node_id、目标为"浩宇"的 DispatchAgentTasks。
///   3. 读取错误码、retryable 与消息内容，并对比 coordinator 任务数。
///   4. 换新 operation_id 再次提交，确认仍被拒绝且不能绕过运行时防线。
///
/// 预期结果：
///   - 正断言：错误码为 SelfDispatchForbidden 且 retryable=false。
///   - 正断言：错误消息包含规范名"浩宇"与"不要更换 operation_id"。
///   - 正断言：两次提交前后 coordinator 任务数不变（零副作用）。
///   - 负断言：新 operation_id 不得绕过自派发拒绝。
#[test]
fn e2e_12_agent_self_dispatch_rejected_with_identity_and_zero_side_effects() {
    let fixture = tempfile::tempdir().expect("自派发 e2e 应创建隔离根目录");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("自派发 e2e 应创建 Workspace 目录");
    let database_paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let socket_path = fixture.path().join("norma-e2e.sock");
    let mut test_cx = TestAppContext::single();

    let (window, app) = test_cx.update(|cx| {
        ui::init_components(cx);
        app::install_norma_endpoint_for_e2e(socket_path, cx)
            .expect("自派发 e2e 应成功安装隔离 Norma endpoint");
        let mut app = None;
        let window = cx
            .open_window(WindowOptions::default(), |window, cx| {
                let entity = cx.new(|cx| {
                    MultisoulApp::new_with_pty_factory_and_database_paths(
                        cx,
                        crate::harness::echo_factory(),
                        database_paths,
                    )
                });
                app = Some(entity.clone());
                cx.new(|cx| Root::new(entity, window, cx))
            })
            .expect("自派发 e2e 应打开唯一 Workspace 窗口");
        (
            window,
            app.expect("Workspace 窗口必须创建 MultisoulApp entity"),
        )
    });

    let source_node_id = prepare_source_agent(&window, &app, &mut test_cx, &workspace);
    test_cx.run_until_parked();
    test_cx.update(app::mark_orchestration_health_ready_for_e2e);
    app.update(&mut test_cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            fixture.path().join("authorization.sqlite"),
            "codex",
            "self-dispatch-token",
        )
        .expect("自派发 e2e 应安装隔离 Codex 授权");
        assert!(
            app.register_external_agent_root_for_e2e(
                source_node_id,
                Some("用户交给浩宇的根任务".to_string()),
            ),
            "浩宇必须注册可信根任务，生产编排入口才能接受子委派"
        );
    });
    let (workspace_id, route_id) = test_cx.update(|cx| {
        app.read(cx)
            .norma_workspace_identity_for_e2e()
            .expect("自派发 Workspace 必须注册稳定 route")
    });

    let task_count_before =
        test_cx.update(|cx| app.read(cx).agent_coordinator_task_count_for_e2e());

    let rejected = submit(
        WireRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            request_id: 301,
            operation_id: Some("aaaaaaaa-0000-4ccc-8ddd-eeeeeeeeeeee".to_string()),
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "self-dispatch-token".to_string(),
            }),
            source_workspace_id: Some(workspace_id.clone()),
            source_workspace_route_id: Some(route_id.clone()),
            target_workspace_id: None,
            source_node_id: Some(source_node_id),
            command: CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "浩宇".to_string(),
                    task: "把支付模块检查完".to_string(),
                }],
                force_untracked: false,
            },
        },
        &mut test_cx,
    );

    let Err(CardCommandError::Agent(failure)) = rejected.result else {
        panic!("浩宇自派发必须返回 Agent 失败，实际：{:?}", rejected.result);
    };
    assert_eq!(
        failure.code,
        AgentErrorCode::SelfDispatchForbidden,
        "浩宇自派发必须返回稳定的 SelfDispatchForbidden 码"
    );
    assert!(!failure.retryable, "确定性拒绝 retryable 必须为 false");
    assert!(
        failure.message.contains("浩宇"),
        "拒绝消息必须包含来源规范名 浩宇，实际：{}",
        failure.message
    );
    assert!(
        failure.message.contains("不要更换 operation_id"),
        "拒绝消息必须明确禁止更换 operation_id，实际：{}",
        failure.message
    );

    let task_count_after = test_cx.update(|cx| app.read(cx).agent_coordinator_task_count_for_e2e());
    assert_eq!(
        task_count_after, task_count_before,
        "自派发被拒后不得新增任何 coordinator 任务；before={task_count_before} after={task_count_after}"
    );

    let rejected_again = submit(
        WireRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            request_id: 302,
            operation_id: Some("bbbbbbbb-0000-4ccc-8ddd-eeeeeeeeeeee".to_string()),
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "self-dispatch-token".to_string(),
            }),
            source_workspace_id: Some(workspace_id),
            source_workspace_route_id: Some(route_id),
            target_workspace_id: None,
            source_node_id: Some(source_node_id),
            command: CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "浩宇".to_string(),
                    task: "换个 operation_id 重试".to_string(),
                }],
                force_untracked: false,
            },
        },
        &mut test_cx,
    );
    assert!(
        matches!(
            rejected_again.result,
            Err(CardCommandError::Agent(ref failure))
                if failure.code == AgentErrorCode::SelfDispatchForbidden
        ),
        "新 operation_id 不得绕过自派发拒绝，实际：{:?}",
        rejected_again.result
    );

    app.update(&mut test_cx, |app, _cx| app.runtime.kill_all());
}

fn prepare_source_agent(
    window_handle: &TestWindow,
    app: &Entity<MultisoulApp>,
    test_cx: &mut TestAppContext,
    workspace: &std::path::Path,
) -> u64 {
    window_handle
        .update(test_cx, |_root, window, cx| {
            app.update(cx, |app, cx| {
                app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx);
                let created = app
                    .execute_agent_card_command_for_e2e(
                        window,
                        CardCommand::CreateAgent {
                            name: Some("浩宇".to_string()),
                            profile: AgentProfile::Codex,
                            activate: true,
                        },
                    )
                    .expect("自派发 e2e 应创建并激活浩宇");
                let CardCommandResult::AgentCreated {
                    node_id: Some(source_node_id),
                    ..
                } = created
                else {
                    panic!("浩宇必须返回 active node_id");
                };
                source_node_id
            })
        })
        .expect("自派发 Workspace 窗口应保持可更新")
}

fn submit(request: WireRequest, test_cx: &mut TestAppContext) -> WireResponse {
    let receiver = test_cx.update(|cx| app::submit_norma_request_for_e2e(request, cx));
    test_cx.run_until_parked();
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("自派发编排请求应在 2 秒测试预算内返回")
}
