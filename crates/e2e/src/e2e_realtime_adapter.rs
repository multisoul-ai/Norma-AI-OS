use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::AgentProfile;

/// 内部实时派工复用手机持久回执：权限、画布、任务记录与重放都指向同一次执行。
///
/// 数据构造：独占项目 0 卡片，phone-realtime 获 control revision=1；一次新建带任务的 Codex。
///   预期 1 次 reserve → 1 卡片 + 1 任务；同 operation 重放仍为 1，改参数产生冲突。
/// 执行过程：注册真实项目路由 → 内部提交 → WSS 查持久回执 → 重读两份 SQLite 数据 → 重放与改参。
/// 预期结果：回执成功前画布与准确任务原文已落盘；任务状态仍是 accepted，不能声称完成；
///   重放不增加卡片或任务，冲突无副作用。Provider 使用 echo PTY，不连接外部模型。
/// 故障分支：仅测试库的 INSERT trigger 拒绝第二个任务；flush 仍会成功，必须回读识别缺失。
///   预期第一条任务保留、第二条没有持久记录；返回 outcome_unknown，重试不产生第三个任务。
/// 首次派工分支：空项目没有名为 codex 的已启用 Agent；先派给它应明确拒绝且不产生卡片/任务，
///   原操作重放仍拒绝，随后另一个明确的新建操作可以正常接收。
#[test]
fn realtime_adapter_receipt_requires_persisted_task_and_canvas() {
    use crate::e2e_mobile_snapshot::fetch_action;
    use app::{MobileAccess, MobileCommand, MobileCommandRequest, MobileServer};
    use serde_json::json;
    use storage::{AppStore, MobilePermission, MobileProjectAccess};
    let mut test = TestAppContext::single();
    let socket_dir = tempfile::tempdir().expect("独占路由目录");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket_dir.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let workspace = tempfile::tempdir().expect("独占项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace.path(), window, cx)
        })
    });
    let (path, snapshots, commands, project_store) = app.read_with(cx, |app, cx| {
        (
            app.app_database_path_for_e2e(),
            app.mobile_snapshot_source_for_e2e(cx),
            app.mobile_command_source_for_e2e(cx),
            app.project_store_for_e2e(workspace.path()).expect("项目库"),
        )
    });
    let store = AppStore::new(path);
    let project = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "Realtime")
        .expect("项目身份")
        .workspace_id;
    let token = "a".repeat(64); // 32 random bytes are encoded as 64 hex characters by the paired-device contract.
    store
        .mobile_access_store()
        .register(
            "phone-realtime",
            "iPhone",
            &token,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("隔离控制授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("隔离TLS身份");
    let certificate = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("正式命令通道");
    let snapshot = fetch_action(
        cx,
        &server,
        &certificate,
        &machine,
        json!({"type":"snapshot", "device_id":"phone-realtime", "token":token, "workspace_id":project}),
    );
    assert_eq!(
        snapshot["result"]["projects"][0]["state"], "open",
        "派工前真实项目路由必须可读取，实际响应: {snapshot}"
    );
    let mut request = MobileCommandRequest {
        operation_id: "realtime-operation".into(),
        workspace_id: project.clone(),
        route_id: snapshot["result"]["projects"][0]["route_id"]
            .as_str()
            .expect("当前路由")
            .into(),
        grant_revision: 1, // Initial register creates revision 1; no permission upgrade is hidden here.
        command: MobileCommand::RealtimeTool {
            command: AdapterCommand::CreateAgent {
                profile: AgentProfile::Codex,
                task: Some("实现登录校验，保留约束并交付测试证据".into()),
            },
        },
    };
    let mut missing = request.clone();
    missing.operation_id = "realtime-missing-agent".into();
    missing.command = MobileCommand::RealtimeTool {
        command: AdapterCommand::AssignAgentTask {
            agent: "codex".into(),
            task: "新建 result.txt，内容仅为云端验证完成".into(),
        },
    };
    server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, missing.clone())
        .expect("持久接收首次派工操作");
    // 5s = 2s foreground allowance + SQLite/TLS scheduling margin; no Provider is started.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let rejected = loop {
        let value = fetch_action(
            cx,
            &server,
            &certificate,
            &machine,
            json!({"type":"receipt", "device_id":"phone-realtime", "token":token,
                "workspace_id":project, "operation_id":missing.operation_id}),
        );
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "不存在的Agent必须有界返回明确拒绝"
        );
    };
    assert_eq!(
        rejected["result"]["receipt"]["state"], "rejected",
        "未进入Adapter且没有目标Agent时不能声称执行结果未知"
    );
    assert_eq!(
        rejected["result"]["receipt"]["error"], "agent_not_found",
        "真实模型必须知道首次派工没有目标，才能新建正确Agent"
    );
    assert!(
        rejected["result"]["receipt"]["result"].is_null(),
        "拒绝不能伪造任务回执"
    );
    assert_eq!(
        read_runtime(&app, cx, |runtime| runtime.document.nodes().len()),
        0,
        "明确拒绝不能偷偷创建卡片"
    );
    assert_eq!(
        app.update(cx, |app, _| app.agent_task_count_for_e2e()),
        0,
        "不存在的Agent不能留下任务"
    );
    let missing_replay = server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, missing)
        .expect("核对首次派工原回执");
    assert_eq!(
        missing_replay["receipt"]["state"], "rejected",
        "原拒绝不能重试成新任务"
    );
    let accepted = server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, request.clone())
        .expect("内部提交");
    assert_eq!(
        accepted["receipt"]["state"], "accepted",
        "尚未驱动GPUI时只能接收，不能宣称已派工"
    );
    let query = json!({"type":"receipt", "device_id":"phone-realtime", "token":token,
        "workspace_id":project, "operation_id":request.operation_id});
    // 5s = 2s foreground allowance + 360ms save debounce + SQLite/TLS scheduling margin.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let finished = loop {
        let value = fetch_action(cx, &server, &certificate, &machine, query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "实时派工回执必须有界确认"
        );
    };
    let receipt = &finished["result"]["receipt"];
    assert_eq!(
        receipt["state"], "succeeded",
        "必须在画布和任务记录落盘后确认命令"
    );
    assert_eq!(
        receipt["result"]["status"], "accepted",
        "工具接收任务不能升级为任务完成"
    );
    let node = receipt["result"]["node_id"].as_u64().expect("准确节点身份");
    let task = receipt["result"]["task_id"].as_u64().expect("准确任务身份");
    let canvas = project_store
        .canvas_store()
        .load_snapshot()
        .expect("重读画布")
        .expect("画布已保存");
    let tasks = project_store
        .agent_dispatch_task_store()
        .list()
        .expect("重读任务库");
    assert_eq!(canvas.nodes.len(), 1, "回执前应保存唯一的Agent卡片");
    assert_eq!(canvas.nodes[0].id, node, "保存的节点必须与回执相同");
    assert_eq!(tasks.len(), 1, "只能保存一次任务");
    assert_eq!(
        tasks[0].runtime_task_id, task,
        "不能用另一任务的持久记录证明当前派工"
    );
    assert_eq!(
        tasks[0].task_text, "实现登录校验，保留约束并交付测试证据",
        "准确任务与约束必须持久化"
    );
    assert_ne!(
        tasks[0].state,
        storage::AgentDispatchTaskState::Done,
        "CLI Ready之前不能报告任务完成"
    );
    let replay = server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, request.clone())
        .expect("只查原回执");
    assert_eq!(
        replay["receipt"]["result"], receipt["result"],
        "重放必须沿用同一节点与任务回执"
    );
    request.command = MobileCommand::RealtimeTool {
        command: AdapterCommand::CreateTerminal,
    };
    assert_eq!(
        server
            .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, request.clone())
            .unwrap_err(),
        "idempotency_conflict",
        "同operation改参数必须拒绝，不能创建终端"
    );
    assert_eq!(
        read_runtime(&app, cx, |runtime| runtime.document.nodes().len()),
        1,
        "重放与冲突都不能增加卡片"
    );
    assert_eq!(
        app.update(cx, |app, _| app.agent_task_count_for_e2e()),
        1,
        "重放不能重复派工"
    );
    rusqlite::Connection::open(project_store.path()).expect("仅打开本测试的项目库")
        .execute_batch("CREATE TRIGGER fail_realtime_dispatch BEFORE INSERT ON agent_dispatch_tasks
            WHEN NEW.task_text = '故障注入任务' BEGIN SELECT RAISE(FAIL, 'fixture write failure'); END;")
        .expect("只拒绝指定的新任务，不删除历史数据");
    request.operation_id = "realtime-failed-write".into();
    request.command = MobileCommand::RealtimeTool {
        command: AdapterCommand::CreateAgent {
            profile: AgentProfile::Claude,
            task: Some("故障注入任务".into()),
        },
    };
    server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, request.clone())
        .expect("操作本身先持久接收");
    let failed_query = json!({"type":"receipt", "device_id":"phone-realtime", "token":token,
        "workspace_id":project, "operation_id":request.operation_id});
    // Same 5s budget: foreground + 360ms debounce + SQLite/TLS margin; failure cannot wait forever.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let unknown = loop {
        let value = fetch_action(cx, &server, &certificate, &machine, failed_query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "写入失败应有界返回未知结果"
        );
    };
    assert_eq!(
        unknown["result"]["receipt"]["state"], "outcome_unknown",
        "队列barrier通过不能掩盖实际写入失败"
    );
    assert!(
        unknown["result"]["receipt"]["result"].is_null(),
        "未知结果不能附带成功派工回执"
    );
    let records = project_store
        .agent_dispatch_task_store()
        .list()
        .expect("故障后仍可读取历史");
    assert_eq!(records.len(), 1, "第二条任务没有落盘，不能用第一条冒充证明");
    assert_eq!(
        records[0].runtime_task_id, task,
        "第一条成功任务的记录应保持完整"
    );
    let replay = server
        .submit_internal_command_for_e2e(&machine, "phone-realtime", &token, request)
        .expect("未知操作只回查");
    assert_eq!(
        replay["receipt"]["state"], "outcome_unknown",
        "未知结果重试不能重新执行"
    );
    assert_eq!(
        app.update(cx, |app, _| app.agent_task_count_for_e2e()),
        2,
        "只有原任务和写入失败的已执行操作，不能产生第三次副作用"
    );
}

/// 实时入口无 Window：复用同一项目的 Agent 卡片、任务队列与明确取消。
///
/// 数据构造：空项目 0 卡片；一次 create_agent(task) → 1 卡片 + 1 任务。
///   查询不增加任务；取消尚未提交的 task_id=1 → 未结束任务由 1 减到 0。
/// 执行过程：1. 建立 headless 项目，所有 Adapter 操作都不传 Window。
///   2. 新建 Codex 并派工。3. 查询真实任务。4. 明确取消该任务后重新查询。
///
/// 预期结果：真实 accepted 和任务身份保留；查询不重复派工；取消保留原卡片，
/// 已取消任务不再作为未结束任务返回。此测试不等于真实 .app 关窗验收。
#[test]
fn realtime_adapter_dispatches_and_queries_without_a_window() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace = tempfile::TempDir::new().expect("创建独占项目");
    enter_canvas(&app, cx, workspace.path());
    let created = app
        .update(cx, |app, cx| {
            app.execute_background_norma_adapter_command_for_e2e(
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                    task: Some("实现已讨论的登录校验并给出测试证据".into()),
                },
            )
        })
        .expect("没有 Window 也应复用正式派工入口");
    assert_eq!(created.status, "accepted", "入队不能被改写成任务完成");
    assert_eq!(created.task_id, Some(1), "必须返回真实的第一条任务身份");
    let node = created.node_id.expect("回执必须包含 Agent 卡片");
    assert!(
        app.update(cx, |app, _| app.agent_submission_waiting_for_e2e(node)),
        "无窗口也必须等待 CLI Ready，不能绕过提交安全门"
    );
    let query = || AdapterCommand::FindAgentTasks {
        task_id: None,
        agent: None,
        states: vec![],
    };
    let before = app
        .update(cx, |app, cx| {
            app.execute_background_norma_adapter_command_for_e2e(cx, query())
        })
        .expect("无窗口查询应成功");
    assert_eq!(before.tasks.len(), 1, "查询只应返回同一个未结束任务");
    assert_eq!(before.tasks[0]["task_id"], 1, "查询必须返回真实已接收任务");
    assert_eq!(
        app.update(cx, |app, _| app.agent_task_count_for_e2e()),
        1,
        "查询不能重复派工"
    );
    let cancelled = app
        .update(cx, |app, cx| {
            app.execute_background_norma_adapter_command_for_e2e(
                cx,
                AdapterCommand::CancelAgentTask { task_id: 1 },
            )
        })
        .expect("明确取消不依赖 Window");
    assert_eq!(cancelled.status, "succeeded", "未提交任务应立即取消");
    assert!(
        !app.update(cx, |app, _| app.agent_submission_waiting_for_e2e(node)),
        "取消后不得继续等待提交该任务"
    );
    let after = app
        .update(cx, |app, cx| {
            app.execute_background_norma_adapter_command_for_e2e(cx, query())
        })
        .expect("取消后仍可查询");
    assert!(after.tasks.is_empty(), "取消任务不能再被报告为未结束");
    let cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });
    assert_eq!(cards, 1, "查询和取消应复用原卡片，不能删除或新建卡片");
}
