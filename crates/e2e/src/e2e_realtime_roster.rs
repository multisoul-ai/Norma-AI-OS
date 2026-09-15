use crate::{echo_factory, make_app, read_runtime};
use app::{MobileAccess, MobileServer};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand};
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 语音概况所用的授权快照包含未打开卡片的项目 Agent，不能把零卡片误报为零 Agent。
///
/// 数据构造：同一项目定义 1 个 Codex（开发甲），activate=false，所以 Agent=1、卡片=0、任务=0。
///   另一个项目存在但 phone-roster 没有权限，不能通过概况读出该项目或任何名单。
/// 执行过程：真实 GPUI 路由创建定义 → TLS 配对授权快照 → 未授权项目读取 → 再检查 Runtime。
/// 预期结果：名单含准确名称、enabled=true、active_node_id=null；空卡片保持空，不启动 PTY 或派工。
#[test]
fn realtime_snapshot_includes_unopened_agents_without_starting_them() {
    let mut test = TestAppContext::single();
    let sockets = tempfile::tempdir().expect("独占路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(sockets.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let workspace = tempfile::tempdir().expect("项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace.path(), window, cx);
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("开发甲".into()),
                    profile: AgentProfile::Codex,
                    activate: false,
                },
            )
            .expect("仅创建定义，不激活卡片");
        });
    });
    let (path, snapshots) = app.read_with(cx, |app, cx| {
        (
            app.app_database_path_for_e2e(),
            app.mobile_snapshot_source_for_e2e(cx),
        )
    });
    let store = AppStore::new(path);
    let project = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "项目甲")
        .expect("项目身份")
        .workspace_id;
    let other = tempfile::tempdir().expect("另一个项目");
    let other_id = store
        .workspace_identity_store()
        .resolve_or_create(other.path(), "未授权项目乙")
        .expect("另一项目身份")
        .workspace_id;
    // 与配对协议相同：32 个随机字节以 64 个十六进制字符传输；本机测试固定其内容。
    let token = "a".repeat(64);
    store
        .mobile_access_store()
        .register(
            "phone-roster",
            "语音手机",
            &token,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("当前项目授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS 身份");
    let certificate = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        None,
    )
    .expect("真实快照服务");
    let read = |cx: &mut _, workspace: &str| {
        crate::e2e_mobile_snapshot::fetch_action(
            cx,
            &server,
            &certificate,
            &machine,
            json!({"type":"snapshot","device_id":"phone-roster","token":token,"workspace_id":workspace}),
        )
    };
    let snapshot = read(cx, &project);
    let denied = read(cx, &other_id);
    server.shutdown();
    assert_eq!(
        snapshot["result"]["projects"][0]["card_count"], 0,
        "读取名单不能激活卡片"
    );
    assert_eq!(
        snapshot["result"]["agents"][0]["name"], "开发甲",
        "未打开卡片的已有 Agent 不能从名单消失"
    );
    assert_eq!(
        snapshot["result"]["agents"][0]["definition_id"], 1,
        "手机必须拿到项目定义的稳定身份，不能只按显示名称猜测"
    );
    assert_eq!(
        snapshot["result"]["agents"][0]["enabled"], true,
        "执行可用性必须来自原项目定义"
    );
    assert!(
        snapshot["result"]["agents"][0]["active_node_id"].is_null(),
        "未打开不能伪造活动节点"
    );
    assert_eq!(
        denied["error"], "unauthorized",
        "未授权项目不能读取 Agent 名单"
    );
    assert!(
        !snapshot.to_string().contains("未授权项目乙"),
        "本次概况不能混入其他项目"
    );
    assert!(denied.get("result").is_none(), "拒绝响应不能夹带任何名单");
    assert!(
        read_runtime(&app, cx, |runtime| runtime.document.nodes().is_empty()),
        "读取概况后仍不能产生任何卡片"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.agent_task_count_for_e2e()),
        0,
        "读取概况不得派发任务"
    );
}

/// 手机按项目 Agent 的稳定定义创建卡片，不能退化成同名的临时手机卡。
///
/// 数据构造：当前项目仅定义 1 个未打开的 Codex「开发甲」，手机拥有 control revision=1。
/// 执行过程：先从真实快照读取 definition_id，再通过 WSS 创建该定义的卡片；随后重复请求同一定义。
/// 预期结果：首次回执成功，卡片同时保留 definition_id、名称和 mobile_control；定义已激活后第二次明确拒绝，节点不增加。
#[test]
fn mobile_creates_phone_controlled_card_from_project_agent_definition() {
    let mut test = TestAppContext::single();
    let sockets = tempfile::tempdir().expect("独占路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(sockets.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let workspace = tempfile::tempdir().expect("项目");
    let definition_id = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace.path(), window, cx);
            match app
                .execute_agent_card_command_for_e2e(
                    window,
                    CardCommand::CreateAgent {
                        name: Some("开发甲".into()),
                        profile: AgentProfile::Codex,
                        activate: false,
                    },
                )
                .expect("仅创建项目 Agent 定义")
            {
                norma::protocol::CardCommandResult::AgentCreated {
                    definition_id,
                    node_id: None,
                    ..
                } => definition_id,
                other => panic!("未激活定义必须返回 definition_id 且没有节点，实际：{other:?}"),
            }
        })
    });
    let (path, snapshots, commands) = app.read_with(cx, |app, cx| {
        (
            app.app_database_path_for_e2e(),
            app.mobile_snapshot_source_for_e2e(cx),
            app.mobile_command_source_for_e2e(cx),
        )
    });
    let store = AppStore::new(path);
    let project = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "项目甲")
        .expect("项目身份")
        .workspace_id;
    let token = "b".repeat(64);
    store
        .mobile_access_store()
        .register(
            "phone-create-agent",
            "创建卡片手机",
            &token,
            &[MobileProjectAccess {
                workspace_id: project.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("控制授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS 身份");
    let certificate = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("真实命令服务");
    let snapshot = crate::e2e_mobile_snapshot::fetch_action(
        cx,
        &server,
        &certificate,
        &machine,
        json!({"type":"snapshot","device_id":"phone-create-agent","token":token,"workspace_id":project}),
    );
    assert_eq!(
        snapshot["result"]["agents"][0]["definition_id"], definition_id,
        "创建前必须从当前项目快照读取准确的稳定定义"
    );
    let route = snapshot["result"]["projects"][0]["route_id"]
        .as_str()
        .expect("当前项目路由")
        .to_owned();
    let command = |operation: &str| {
        json!({"type":"command","device_id":"phone-create-agent","token":token,"request":{
            "operation_id":operation,"workspace_id":project,"route_id":route,"grant_revision":1,
            "command":{"type":"create_card","profile":"Codex","title":"开发甲","definition_id":definition_id}
        }})
    };
    let accepted = crate::e2e_mobile_snapshot::fetch_action(
        cx,
        &server,
        &certificate,
        &machine,
        command("create-project-agent"),
    );
    assert_eq!(
        accepted["result"]["status"], "receipt",
        "创建应进入持久回执"
    );
    let receipt = |cx: &mut _| {
        crate::e2e_mobile_snapshot::fetch_action(
            cx,
            &server,
            &certificate,
            &machine,
            json!({"type":"receipt","device_id":"phone-create-agent","token":token,
                "workspace_id":project,"operation_id":"create-project-agent"}),
        )
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let created = loop {
        let value = receipt(cx);
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "项目 Agent 卡片必须在保存后有界完成"
        );
    };
    assert_eq!(
        created["result"]["receipt"]["state"], "succeeded",
        "项目定义有效时必须创建成功"
    );
    let node_id = created["result"]["receipt"]["result"]["node_id"]
        .as_u64()
        .expect("准确节点");
    assert!(
        read_runtime(&app, cx, |runtime| {
            matches!(
                runtime.document.node(node_id).map(|node| &node.kind),
                Some(canvas::CanvasNodeKind::Agent(spec))
                    if spec.definition_id == Some(definition_id)
                        && spec.alias.as_deref() == Some("开发甲")
                        && spec.mobile_control
            )
        }),
        "创建结果必须同时绑定项目定义并保留手机控制能力"
    );
    let _ = crate::e2e_mobile_snapshot::fetch_action(
        cx,
        &server,
        &certificate,
        &machine,
        command("duplicate-project-agent"),
    );
    let duplicate_query = json!({"type":"receipt","device_id":"phone-create-agent","token":token,
        "workspace_id":project,"operation_id":"duplicate-project-agent"});
    let duplicate = loop {
        let value = crate::e2e_mobile_snapshot::fetch_action(
            cx,
            &server,
            &certificate,
            &machine,
            duplicate_query.clone(),
        );
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "重复激活必须有界返回拒绝"
        );
    };
    assert_eq!(
        duplicate["result"]["receipt"]["state"], "rejected",
        "同一项目定义已激活后不能再创建第二张卡"
    );
    assert_eq!(
        duplicate["result"]["receipt"]["error"], "create_failed",
        "定义不可用时应返回稳定错误"
    );
    assert_eq!(
        read_runtime(&app, cx, |runtime| runtime.document.nodes().len()),
        1,
        "拒绝重复激活后节点数量不能增加"
    );
    server.shutdown();
    app.update(cx, |app, _| app.runtime.kill_all());
}
