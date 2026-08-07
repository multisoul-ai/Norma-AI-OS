//! Agent CLI 同 Workspace 指派的流星来源端到端验收。

use app::MultisoulApp;
use gpui::{AppContext as _, Entity, TestAppContext, WindowHandle, WindowOptions};
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
    ProviderAuthorization, WireRequest, WireResponse, CURRENT_PROTOCOL_VERSION,
};
use ui::component::Root;

type TestWindow = WindowHandle<Root>;

/// DR-A2A-E2E-1：带 source_node_id 的 Norma Agent CLI 指派必须生成来源卡片到目标卡片的流星。
///
/// 数据构造（含关键数值的推导过程）：
///   Workspace windows = 1
///   Agent cards = 浩宇/Codex(1) + 老乔/Claude(1) = 2
///   trusted source roots = 1（浩宇当前用户根任务）
///   CLI wire requests = 1；assignments = 1
///   expected business receipts = 1；expected active visual effects = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装隔离的 App-global Norma endpoint，并在同一 Workspace 创建浩宇、老乔。
///   2. 为浩宇安装 Provider 授权并注册可信外部根任务。
///   3. 通过生产 WireRequest 提交带浩宇 source_node_id 的 DispatchAgentTasks。
///   4. 读取业务 receipt 与公开 dispatch effect e2e snapshot。
///
/// 预期结果：
///   - 正断言：业务层返回唯一 Queued receipt，目标精确为老乔。
///   - 正断言：视觉层生成唯一 Flight，来源精确为浩宇、目标精确为老乔。
///   - 负断言：Agent CLI 指派不得退化为 Command Dock 来源，也不得误命中浩宇。
#[test]
fn e2e_12_agent_cli_dispatch_spawns_source_agent_to_target_agent_meteor() {
    let fixture = tempfile::tempdir().expect("Agent-to-Agent e2e 应创建隔离根目录");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("Agent-to-Agent e2e 应创建 Workspace 目录");
    let database_paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let socket_path = fixture.path().join("norma-e2e.sock");
    let mut test_cx = TestAppContext::single();

    let (window, app) = test_cx.update(|cx| {
        ui::init_components(cx);
        app::install_norma_endpoint_for_e2e(socket_path, cx)
            .expect("Agent-to-Agent e2e 应成功安装隔离 Norma endpoint");
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
            .expect("Agent-to-Agent e2e 应打开唯一 Workspace 窗口");
        (
            window,
            app.expect("Workspace 窗口必须创建 MultisoulApp entity"),
        )
    });

    let (source_node_id, target_node_id) =
        prepare_workspace_agents(&window, &app, &mut test_cx, &workspace);
    test_cx.run_until_parked();
    test_cx.update(app::mark_orchestration_health_ready_for_e2e);
    app.update(&mut test_cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            fixture.path().join("authorization.sqlite"),
            "codex",
            "agent-handoff-token",
        )
        .expect("来源 Workspace 应安装隔离 Codex 授权");
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
            .expect("Agent-to-Agent Workspace 必须注册稳定 route")
    });

    let assigned = submit(
        WireRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            request_id: 201,
            operation_id: Some("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee".to_string()),
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "agent-handoff-token".to_string(),
            }),
            source_workspace_id: Some(workspace_id),
            source_workspace_route_id: Some(route_id),
            target_workspace_id: None,
            source_node_id: Some(source_node_id),
            command: CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "老乔".to_string(),
                    task: "检查当前流星效果实现".to_string(),
                }],
                force_untracked: false,
            },
        },
        &mut test_cx,
    );
    let Ok(CardCommandResult::AgentTasksDispatched { receipts }) = assigned.result else {
        panic!("Agent CLI 同 Workspace 指派必须返回任务回执");
    };
    let receipt = receipts
        .first()
        .expect("单目标 Agent CLI 指派必须返回且只需读取一条 receipt");
    let snapshot = app.update(&mut test_cx, |app, _cx| {
        app.dispatch_receipt_effect_e2e_snapshot()
    });

    assert_eq!(
        receipts.len(),
        1,
        "一条 Agent CLI assignment 必须精确产生一条业务 receipt"
    );
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "老乔尚未 ready 时业务回执必须保持真实 Queued 状态"
    );
    assert_eq!(
        receipt.node_id, target_node_id,
        "业务 receipt 必须精确命中老乔节点"
    );
    assert_eq!(
        snapshot.active_count, 1,
        "唯一成功 Agent CLI 指派必须创建且只创建一个 active visual effect"
    );
    assert_eq!(
        snapshot.effects[0].origin, "agent",
        "生产 WireRequest 的流星来源类型必须是 Agent"
    );
    assert_eq!(
        snapshot.effects[0].origin_node_id,
        Some(source_node_id),
        "生产 WireRequest 的流星必须精确从浩宇节点发射"
    );
    assert_eq!(
        snapshot.effects[0].target_node_id, target_node_id,
        "生产 WireRequest 的流星必须精确命中老乔节点"
    );
    assert_eq!(
        snapshot.effects[0].phase, "flight",
        "非 Reduce Motion 的 Agent-to-Agent 流星必须从 Flight 开始"
    );
    assert_ne!(
        snapshot.effects[0].origin, "dock",
        "Agent CLI 指派不得伪装成 Command Dock 发射"
    );
    assert_ne!(
        snapshot.effects[0].target_node_id, source_node_id,
        "Agent-to-Agent 流星不得误命中来源浩宇"
    );

    app.update(&mut test_cx, |app, _cx| app.runtime.kill_all());
}

/// DR-A2A-E2E-2：飞行中来源 Agent 离屏必须取消视觉槽，但不得撤销已经接受的业务任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Workspace windows = 1；Agent cards = 2
///   accepted assignments = 1
///   business task count = external root(1) + delegated child(1) = 2
///   source offscreen rect = (-10_000, -10_000, 240, 160)
///   source center = (-9_880, -9_920)，远小于 viewport 左上角 (0, 0)
///   active effects before pan = 1；after pan = 0；cancelled effects = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过生产 WireRequest 从浩宇向老乔派发任务并确认 Flight 已创建。
///   2. 保留目标老乔可见，仅把来源浩宇的当前视觉矩形移出 viewport。
///   3. 通知 GPUI 重绘，让可见端点清理逻辑检查来源与目标。
///   4. 对比业务任务计数、effect slot、取消计数与目标节点。
///
/// 预期结果：
///   - 正断言：离屏前存在唯一浩宇→老乔 Flight，离屏后精确取消一次。
///   - 正断言：业务 coordinator 仍保留已经接受的唯一任务，老乔节点仍存在。
///   - 负断言：来源离屏后不得保留不可见 Flight/Impact，也不得误删目标老乔。
#[test]
fn e2e_12_agent_handoff_source_offscreen_cancels_visual_only() {
    let fixture = tempfile::tempdir().expect("来源离屏 e2e 应创建隔离根目录");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("来源离屏 e2e 应创建 Workspace 目录");
    let database_paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let socket_path = fixture.path().join("norma-e2e.sock");
    let mut test_cx = TestAppContext::single();

    let (window, app) = test_cx.update(|cx| {
        ui::init_components(cx);
        app::install_norma_endpoint_for_e2e(socket_path, cx)
            .expect("来源离屏 e2e 应成功安装隔离 Norma endpoint");
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
            .expect("来源离屏 e2e 应打开唯一 Workspace 窗口");
        (
            window,
            app.expect("来源离屏 Workspace 必须创建 MultisoulApp entity"),
        )
    });
    let (source_node_id, target_node_id) =
        prepare_workspace_agents(&window, &app, &mut test_cx, &workspace);
    test_cx.run_until_parked();
    test_cx.update(app::mark_orchestration_health_ready_for_e2e);
    app.update(&mut test_cx, |app, _cx| {
        app.install_presentation_authorization_for_e2e(
            fixture.path().join("authorization.sqlite"),
            "codex",
            "offscreen-source-token",
        )
        .expect("来源离屏 Workspace 应安装隔离 Codex 授权");
        assert!(
            app.register_external_agent_root_for_e2e(
                source_node_id,
                Some("用户交给浩宇的根任务".to_string()),
            ),
            "来源离屏前浩宇必须注册可信根任务"
        );
    });
    let (workspace_id, route_id) = test_cx.update(|cx| {
        app.read(cx)
            .norma_workspace_identity_for_e2e()
            .expect("来源离屏 Workspace 必须注册稳定 route")
    });
    let assigned = submit(
        WireRequest {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            request_id: 202,
            operation_id: Some("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff".to_string()),
            authorization: Some(ProviderAuthorization {
                provider: AgentProfile::Codex,
                token: "offscreen-source-token".to_string(),
            }),
            source_workspace_id: Some(workspace_id),
            source_workspace_route_id: Some(route_id),
            target_workspace_id: None,
            source_node_id: Some(source_node_id),
            command: CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "老乔".to_string(),
                    task: "来源离屏后继续保留业务任务".to_string(),
                }],
                force_untracked: false,
            },
        },
        &mut test_cx,
    );
    assert!(
        matches!(
            assigned.result,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].node_id == target_node_id
        ),
        "来源离屏前生产指派必须返回唯一且目标为老乔的业务 receipt"
    );
    let (before, task_count_before) = app.update(&mut test_cx, |app, _cx| {
        (
            app.dispatch_receipt_effect_e2e_snapshot(),
            app.agent_task_count_for_e2e(),
        )
    });
    assert_eq!(
        before.active_count, 1,
        "来源离屏前必须存在唯一浩宇→老乔 active effect"
    );
    assert_eq!(
        before.effects[0].origin_node_id,
        Some(source_node_id),
        "离屏前 Flight 来源必须精确为浩宇"
    );
    assert_eq!(
        task_count_before, 2,
        "可信外部根任务 1 条 + 已接受子委派 1 条，业务 coordinator 应精确保留 2 条任务"
    );

    app.update(&mut test_cx, |app, cx| {
        app.set_node_visual_rect_for_e2e(
            source_node_id,
            canvas::CanvasRect::new(-10_000.0, -10_000.0, 240.0, 160.0),
        );
        cx.notify();
    });
    test_cx.run_until_parked();
    let (after, task_count, target_still_exists) = app.update(&mut test_cx, |app, _cx| {
        (
            app.dispatch_receipt_effect_e2e_snapshot(),
            app.agent_task_count_for_e2e(),
            app.runtime.document.node(target_node_id).is_some(),
        )
    });

    assert_eq!(
        after.active_count, 0,
        "来源浩宇离屏后 active effect 必须立即归零"
    );
    assert!(
        after.effects.is_empty(),
        "来源浩宇离屏后不得残留不可见 Flight 或悬空 Impact"
    );
    assert_eq!(
        after.cancelled_effects, 1,
        "清理唯一来源离屏 effect 应精确记录一次取消"
    );
    assert_eq!(
        task_count, task_count_before,
        "视觉取消不得改变 coordinator 的外部根任务与已接受子任务计数"
    );
    assert_eq!(
        task_count, 2,
        "视觉取消后仍应保留外部根任务 1 条 + 已接受子委派 1 条"
    );
    assert!(target_still_exists, "视觉取消不得误删仍可见的目标老乔节点");

    app.update(&mut test_cx, |app, _cx| app.runtime.kill_all());
}

fn prepare_workspace_agents(
    window_handle: &TestWindow,
    app: &Entity<MultisoulApp>,
    test_cx: &mut TestAppContext,
    workspace: &std::path::Path,
) -> (u64, u64) {
    window_handle
        .update(test_cx, |_root, window, cx| {
            app.update(cx, |app, cx| {
                app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx);
                let source = app
                    .execute_agent_card_command_for_e2e(
                        window,
                        CardCommand::CreateAgent {
                            name: Some("浩宇".to_string()),
                            profile: AgentProfile::Codex,
                            activate: true,
                        },
                    )
                    .expect("Agent-to-Agent e2e 应创建并激活浩宇");
                let target = app
                    .execute_agent_card_command_for_e2e(
                        window,
                        CardCommand::CreateAgent {
                            name: Some("老乔".to_string()),
                            profile: AgentProfile::Claude,
                            activate: true,
                        },
                    )
                    .expect("Agent-to-Agent e2e 应创建并激活老乔");
                let CardCommandResult::AgentCreated {
                    node_id: Some(source_node_id),
                    ..
                } = source
                else {
                    panic!("浩宇必须返回 active node_id");
                };
                let CardCommandResult::AgentCreated {
                    node_id: Some(target_node_id),
                    ..
                } = target
                else {
                    panic!("老乔必须返回 active node_id");
                };
                (source_node_id, target_node_id)
            })
        })
        .expect("Agent-to-Agent Workspace 窗口应保持可更新")
}

fn submit(request: WireRequest, test_cx: &mut TestAppContext) -> WireResponse {
    let receiver = test_cx.update(|cx| app::submit_norma_request_for_e2e(request, cx));
    test_cx.run_until_parked();
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("Agent CLI 编排请求应在 2 秒测试预算内返回")
}
