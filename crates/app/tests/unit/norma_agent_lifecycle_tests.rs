use super::*;
use crate::canvas::master_layout::MasterLayoutState;
use canvas::{AgentProfile, AgentSpec};
use norma::protocol::{
    AgentErrorCode, AgentProfile as NormaAgentProfile, CardCommandError, ProviderAuthorization,
    WireRequest,
};

/// Norma Agent 生命周期：删除 Agent Card 时执行结果必须携带被绑定的 Definition id。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent Card = 1 张：node_id 由空 document 从 1 开始分配
///   definition_id = 41：模拟快速入口持久化的 Definition
///   删除后节点数 = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 Runtime document 添加绑定 definition_id=41 的 Codex Agent Card。
///   2. 通过 Norma executor 执行 `Delete { node_id }`。
///   3. executor 在删除前读取 AgentSpec，并把 Definition id 带入执行副作用。
///
/// 预期结果：
///   - 正断言：删除返回目标 node_id，并携带 definition_id=41 供 App 清理 QuickCard Definition。
///   - 正断言：删除后 document 为空。
///   - 负断言：Definition id 不得丢失为 None，否则语音/Socket 删除会泄漏临时 Definition。
#[test]
fn test_norma_delete_agent_reports_removed_definition_id() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".to_string()),
            definition_id: Some(41),
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 360.0),
    );

    let execution = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Delete { node_id },
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
    )
    .expect("Norma 应成功删除存在的 Agent Card");

    assert_eq!(
        execution.result,
        CardCommandResult::Deleted { node_id },
        "Norma 删除结果必须指向刚删除的 Agent Card"
    );
    assert_eq!(
        execution.removed_agent_definition_id,
        Some(41),
        "删除 Agent Card 时必须保留 definition_id=41 供上层清理临时 Definition"
    );
    assert!(
        runtime.document.nodes().is_empty(),
        "删除唯一 Agent Card 后 document 应为空"
    );
    assert_ne!(
        execution.removed_agent_definition_id, None,
        "语音或 Socket 删除不得丢失 Agent Definition 生命周期信息"
    );
}

/// Norma Socket 编排鉴权：有效 Provider/来源节点可查询，跨 Provider 与错误 Token 必须拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   活跃来源 Agent = Alan/Codex，node_id 由空画布分配
///   授权记录 = Codex token + Claude token，共 2 条
///   请求数 = 3：有效 Codex、Claude 冒用 Codex 节点、错误 Codex token
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在真实 headless MultisoulApp 打开临时项目并激活 Alan/Codex。
///   2. 后台存储边界加载两条 Token 哈希到 App 内存授权快照。
///   3. 依次通过生产 `execute_card_command` 执行三条 `agent list` 请求。
///
/// 预期结果：
///   - 正断言：有效 Codex 请求返回当前项目 Agent 列表。
///   - 正断言：Provider/节点不匹配返回 source_agent_unavailable，错误 Token 返回 unauthorized。
///   - 负断言：两个非法请求不得退化为成功列表或低层 Unsupported。
#[test]
fn test_orchestration_socket_validates_authorization_and_source_agent_profile() {
    let mut gpui = gpui::TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("Socket 鉴权测试应创建临时项目");
    let app_db = workspace.path().join("app.sqlite");
    let authorization_store = storage::OrchestrationAuthorizationStore::new(app_db);
    authorization_store
        .authorize("codex", "codex-token", "1.0.0")
        .expect("测试应登记 Codex Token");
    authorization_store
        .authorize("claude", "claude-token", "1.0.0")
        .expect("测试应登记 Claude Token");
    let authorization_cache = storage::OrchestrationAuthorizationCache::new(
        authorization_store.list().expect("测试应加载授权哈希快照"),
    );
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_cmd, _cols, _rows, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
            }),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.orchestration_authorizations = authorization_cache;
    });
    let source_node_id = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let created = crate::app::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: NormaAgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("测试应激活 Alan/Codex");
            let CardCommandResult::AgentCreated {
                node_id: Some(node_id),
                ..
            } = created
            else {
                panic!("激活 Alan 应返回 AgentCreated 与 node_id");
            };
            node_id
        })
    });

    let (valid, mismatched, unauthorized) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let valid = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 1,
                    operation_id: None,
                    authorization: Some(ProviderAuthorization {
                        provider: NormaAgentProfile::Codex,
                        token: "codex-token".to_string(),
                    }),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::ListAgents,
                },
                window,
                cx,
            );
            let mismatched = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 2,
                    operation_id: None,
                    authorization: Some(ProviderAuthorization {
                        provider: NormaAgentProfile::Claude,
                        token: "claude-token".to_string(),
                    }),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::ListAgents,
                },
                window,
                cx,
            );
            let unauthorized = app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 3,
                    operation_id: None,
                    authorization: Some(ProviderAuthorization {
                        provider: NormaAgentProfile::Codex,
                        token: "wrong-token".to_string(),
                    }),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(source_node_id),
                    command: CardCommand::ListAgents,
                },
                window,
                cx,
            );
            (valid, mismatched, unauthorized)
        })
    });

    assert!(
        matches!(
            valid.result,
            Ok(CardCommandResult::AgentsListed { ref agents })
                if agents.len() == 1 && agents[0].name == "Alan"
        ),
        "有效 Codex 授权与来源节点必须返回只含当前项目 Alan 的 Agent 列表；实际：{valid:?}"
    );
    assert!(
        matches!(
            mismatched.result,
            Err(CardCommandError::Agent(ref failure))
                if failure.code == AgentErrorCode::SourceAgentUnavailable
        ),
        "Claude 授权不得冒用 Codex 来源节点；实际：{mismatched:?}"
    );
    assert!(
        matches!(
            unauthorized.result,
            Err(CardCommandError::Agent(ref failure))
                if failure.code == AgentErrorCode::Unauthorized
        ),
        "错误 Codex Token 必须返回稳定 unauthorized；实际：{unauthorized:?}"
    );
    assert!(
        mismatched.result.is_err() && unauthorized.result.is_err(),
        "非法编排请求不得返回任何成功结果"
    );
}

/// Agent 子委派生产路径：Socket assign 必须复用协调器并由服务端生成来源、父任务和根任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = Alan/Codex（来源）+ Bella/Claude（目标），共 2 个
///   顶层任务 = runtime:1，来自用户直接输入 Alan 卡片并由 Hook 注册
///   子任务 = runtime:2 由 Alan 派给 Bella
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 激活 Alan 与 Bella，模拟 UserPromptSubmit Hook 注册 Alan 的用户直接任务。
///   2. 用 Alan 的 Codex 授权和 source_node_id 通过 Socket 派发 Bella 子任务。
///   3. 读取真实回执与同一个 AgentTaskCoordinator 的任务树。
///
/// 预期结果：
///   - 正断言：子任务目标为 Bella，parent/root 都等于 Alan 顶层 task_ref。
///   - 正断言：协调器存在两条任务且子任务来源 definition/node 为 Alan。
///   - 负断言：CLI 不得自由提供父任务，也不得建立第二套任务队列。
#[test]
fn test_orchestration_socket_dispatch_reuses_coordinator_and_derives_parent_lineage() {
    let mut gpui = gpui::TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("子委派测试应创建临时项目");
    let authorization_store =
        storage::OrchestrationAuthorizationStore::new(workspace.path().join("app.sqlite"));
    authorization_store
        .authorize("codex", "codex-token", "1.0.0")
        .expect("测试应登记 Codex Token");
    let authorization_cache = storage::OrchestrationAuthorizationCache::new(
        authorization_store
            .list()
            .expect("测试应加载 Codex 授权哈希"),
    );
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_cmd, _cols, _rows, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
            }),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.orchestration_authorizations = authorization_cache;
    });
    let (alan_definition_id, alan_node_id) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let alan = crate::app::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: NormaAgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("测试应激活 Alan");
            crate::app::execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Bella".to_string()),
                    profile: NormaAgentProfile::Claude,
                    activate: true,
                },
            )
            .expect("测试应激活 Bella");
            let CardCommandResult::AgentCreated {
                definition_id,
                node_id: Some(node_id),
                ..
            } = alan
            else {
                panic!("Alan 激活结果必须含 definition_id 与 node_id");
            };
            assert!(
                app.register_external_agent_root(node_id, Some("负责主任务".to_string())),
                "UserPromptSubmit Hook 必须把用户直接任务注册为协调器根任务"
            );
            (definition_id, node_id)
        })
    });

    let response = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_card_command(
                WireRequest {
                    protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
                    client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    request_id: 4,
                    operation_id: Some("operation-4".to_string()),
                    authorization: Some(ProviderAuthorization {
                        provider: NormaAgentProfile::Codex,
                        token: "codex-token".to_string(),
                    }),
                    source_workspace_id: None,
                    source_workspace_route_id: None,
                    target_workspace_id: None,
                    source_node_id: Some(alan_node_id),
                    command: CardCommand::DispatchAgentTasks {
                        assignments: vec![norma::protocol::AgentTaskAssignment {
                            name: "Bella".to_string(),
                            task: "运行测试".to_string(),
                        }],
                        force_untracked: false,
                    },
                },
                window,
                cx,
            )
        })
    });
    let tasks = app.read_with(cx, |app, _cx| {
        app.agent_task_coordinator
            .as_ref()
            .expect("子委派必须复用生产协调器")
            .tasks()
    });
    let receipts = match response.result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts,
        other => panic!("子委派必须返回真实 AgentTasksDispatched 回执；实际：{other:?}"),
    };

    assert_eq!(
        receipts.len(),
        1,
        "单次 CLI assign 必须只生成一条 Bella 子任务回执"
    );
    assert_eq!(
        receipts[0].name, "Bella",
        "子任务回执目标必须是用户明确指定的 Bella"
    );
    assert_eq!(
        receipts[0].parent_task_id.as_deref(),
        Some(tasks[0].task_ref.as_str()),
        "父任务必须由服务端解析为 Alan 当前顶层任务"
    );
    assert_eq!(
        receipts[0].root_task_id, tasks[0].task_ref,
        "首层子委派的根任务必须是 Alan 顶层任务"
    );
    assert_eq!(
        tasks.len(),
        2,
        "顶层任务和子任务必须共存在同一个 AgentTaskCoordinator"
    );
    assert_eq!(
        tasks[1].source_definition_id,
        Some(alan_definition_id),
        "子任务来源 Definition 必须由 source_node_id 反查为 Alan"
    );
    assert_eq!(
        tasks[1].source_node_id,
        Some(alan_node_id),
        "子任务来源节点必须由服务端上下文保存"
    );
    assert_ne!(
        tasks[1].task_ref, tasks[0].task_ref,
        "子任务必须获得唯一 task_ref，不得复用父任务引用"
    );
}
