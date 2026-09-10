use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app},
};
use app::{MobileAccess, MobileServer};
use gpui::TestAppContext;
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 手机卡片沿用原生终端布局和键盘输入，创建时没有任务或会话正文。
///
/// 数据构造：640×400 原生 Claude 卡、80×24 echo PTY；输入一段中文，不发送 Enter。
/// 执行过程：创建原生手机卡 → 正式 GPUI 渲染 → 聚焦原卡 → 中文输入 → 读取原终端。
/// 预期结果：原终端显示中文；不存在开始对话表单和 session gone；只有一个PTY，未伪造会话。
#[test]
fn e2e_mobile_card_uses_original_terminal_and_desktop_input() {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app(&mut test, echo_factory());
    let workspace = tempfile::tempdir().expect("隔离项目");
    crate::harness::enter_canvas(&app, cx, workspace.path());
    let node = app.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_mobile_agent_card(
                canvas::AgentProfile::Claude,
                None,
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("原生卡");
        app.runtime.focus.focus_node(node);
        cx.notify();
        node
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("agent-start-input").is_none(),
        "不得叠加新输入表单"
    );
    assert!(
        cx.debug_bounds("terminal-session-missing").is_none(),
        "原生PTY必须存在"
    );
    cx.simulate_input("手机和电脑使用同一卡片");
    app.update(cx, |app, _| {
        app.runtime.drain_all();
    });
    let grid = crate::harness::read_grid(&app, cx, node).expect("原生终端");
    let text: String = grid
        .rows
        .iter()
        .flat_map(|row| {
            row.iter()
                .filter(|cell| !cell.attrs.wide_char_spacer)
                .map(|cell| cell.c)
        })
        .collect();
    assert!(
        text.contains("手机和电脑使用同一卡片"),
        "中文必须进入原生终端，实际：{text}"
    );
    app.read_with(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "只有一个原生终端");
        assert!(
            app.runtime.agent_session_id(node).is_none(),
            "创建和输入草稿不能伪造Provider会话"
        );
    });
}

/// 手机通过真实 WSS 创建空卡，画布落盘后才成功；同 ID 重试与回查不能重复创建。
///
/// 数据构造：phone-a 最初只读 project-a，权限 revision=1；升级 control 后 revision=2；创建一张 Codex。
/// 扩展中断：只读/旧权限拒绝、旧回合拒绝并持久化、重放沿用拒绝、改回合冲突，不删除卡片。
/// 扩展删除：同项目再加一张 Claude，Codex 绑定已有 quick definition 与历史摘要；旧快照拒绝，准确删除后保留 Claude 和历史。
/// 执行过程：读取真实 route → 只读拒绝 → 升权 → 旧版本拒绝 → 新版本提交 → 回查保存 → 重试 / 改参数 / 旧窗口。
/// 预期结果：SQLite 和画布恰好一张卡、零 Provider；重复操作返回同节点；冲突和旧窗口不产生新节点。
#[test]
fn e2e_mobile_create_card_persists_before_receipt_and_never_replays() {
    let mut cx = TestAppContext::single();
    let socket_dir = tempfile::tempdir().expect("隔离 socket");
    cx.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket_dir.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("项目");
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
    let id = store
        .workspace_identity_store()
        .resolve_or_create(workspace.path(), "Norma")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("WSS");
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id}),
    );
    let route = snapshot["result"]["projects"][0]["route_id"]
        .as_str()
        .expect("真实窗口")
        .to_owned();
    let mut action = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"create-a","workspace_id":id,"route_id":route,"grant_revision":1,
        "command":{"type":"create_card","profile":"Codex","title":"手机新卡"}
    }});
    let denied = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(denied["error"], "unauthorized", "只读不能创建卡片");
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.document.nodes().len()),
        0,
        "只读请求无副作用"
    );
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("显式升级");
    let stale = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(stale["error"], "grant_changed", "旧授权版本不能沿用");
    action["request"]["grant_revision"] = json!(2);
    let submitted = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        submitted["result"]["status"], "receipt",
        "必须取得可回查回执"
    );
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"create-a"});
    // 5s = foreground 2s + debounce 360ms + SQLite/TLS scheduling allowance; no unbounded waits.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        let value = fetch_action(cx, &server, &cert, &machine, query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "创建应在保存后有界完成"
        );
    };
    assert_eq!(
        result["result"]["receipt"]["state"], "succeeded",
        "成功必须晚于画布保存"
    );
    let node_id = result["result"]["receipt"]["result"]["node_id"]
        .as_u64()
        .expect("真实节点");
    let saved = project_store
        .canvas_store()
        .load_snapshot()
        .expect("读取项目库")
        .expect("必须先落盘");
    assert_eq!(saved.nodes.len(), 1, "成功时 SQLite 必须已保存空卡");
    assert_eq!(saved.nodes[0].id, node_id, "回执准确指向保存的卡片");
    assert!(
        matches!(&saved.nodes[0].kind,canvas::CanvasNodeKind::Agent(spec) if spec.mobile_control && spec.alias.as_deref()==Some("手机新卡")),
        "保留受控模式和名称"
    );
    let replay = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        replay["result"]["receipt"]["result"]["node_id"], node_id,
        "重复发送认领原回执"
    );
    action["request"]["command"]["title"] = json!("不可重复");
    let conflict = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        conflict["error"], "idempotency_conflict",
        "同 ID 改参数不能创建第二张"
    );
    action["request"]["operation_id"] = json!("stale-window");
    action["request"]["route_id"] = json!("old-route");
    let _ = fetch_action(cx, &server, &cert, &machine, action);
    let old_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"stale-window"});
    let rejected = loop {
        let value = fetch_action(cx, &server, &cert, &machine, old_query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(std::time::Instant::now() < deadline, "旧窗口应立即拒绝");
    };
    assert_eq!(
        rejected["result"]["receipt"]["error"], "route_expired",
        "不能选择新窗口替代请求的旧窗口"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.document.nodes().len()),
        1,
        "重复和过期请求不得增加卡片"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "创建只应打开一个原生终端，不能留空表单"
    );
    let definition = project_store
        .agent_definition_store()
        .create_quick(&storage::NewAgentDefinition {
            name: "保留历史".into(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("既有定义");
    let tails = project_store.agent_conversation_tail_store();
    tails
        .upsert(&storage::StoredAgentConversationTail {
            definition_id: definition.id,
            node_id,
            session_id: "history-a".into(),
            agent_name: definition.name.clone(),
            profile: "Codex".into(),
            last_user_message: Some("原任务".into()),
            last_agent_message: Some("已完成".into()),
            updated_at_unix: 0,
            availability: storage::StoredConversationAvailability::Available,
            user_message_truncated: false,
            agent_message_truncated: false,
        })
        .expect("原历史，timestamp=0 使用 store 当前时间");
    let retained = app.update(cx, |app, _| {
        let hook = norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            node_id,
            json!({"hook_event_name":"SessionStart","session_id":"session-ready"}),
        )
        .expect("fixture Ready");
        assert!(
            app.runtime.observe_mobile_hook(&hook),
            "echo PTY 的明确测试启动事件"
        );
        app.runtime
            .apply_agent_task_update_with_outcome(runtime::AgentTaskUpdate {
                node_id,
                profile: canvas::AgentProfile::Codex,
                provider_event: "SessionStart".into(),
                to: runtime::AgentTaskState::Ready,
                task_title: None,
                session_id: Some("session-ready".into()),
            });
        if let canvas::CanvasNodeKind::Agent(spec) =
            &mut app.runtime.document.node_mut(node_id).expect("目标").kind
        {
            spec.definition_id = Some(definition.id);
        }
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Claude,
                None,
                None,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("保留卡，640×400 为标准占位")
    });
    let expected = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let mut deletion = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"delete-stale","workspace_id":id,"route_id":route,"grant_revision":2,
        "command":{"type":"delete_card","expected":expected,"confirm_busy":false}
    }});
    deletion["request"]["command"]["expected"]["title"] = json!("旧名称");
    let _ = fetch_action(cx, &server, &cert, &machine, deletion.clone());
    let stale_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"delete-stale"});
    let stale = wait_receipt(cx, &server, &cert, &machine, stale_query);
    assert_eq!(
        stale["result"]["receipt"]["error"], "card_changed",
        "旧快照不得删除变化后的卡片"
    );
    assert!(
        app.read_with(cx, |app, _| app.runtime.document.node(node_id).is_some()),
        "拒绝后目标保留"
    );
    deletion["request"]["operation_id"] = json!("delete-a");
    deletion["request"]["command"]["expected"] = json!(expected);
    let _ = fetch_action(cx, &server, &cert, &machine, deletion.clone());
    let delete_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"delete-a"});
    let deleted = wait_receipt(cx, &server, &cert, &machine, delete_query);
    assert_eq!(
        deleted["result"]["receipt"]["state"], "succeeded",
        "删除必须保存后成功"
    );
    assert_eq!(
        deleted["result"]["receipt"]["result"]["node_id"], node_id,
        "删除回执绑定原卡"
    );
    let saved = project_store
        .canvas_store()
        .load_snapshot()
        .expect("读取")
        .expect("有画布");
    assert!(
        !saved.nodes.iter().any(|node| node.id == node_id),
        "删除成功后 SQLite 不能有 Codex"
    );
    assert!(
        saved.nodes.iter().any(|node| node.id == retained),
        "同项目 Claude 必须保留"
    );
    assert_eq!(
        tails
            .get(definition.id, "history-a")
            .expect("读取历史")
            .expect("历史仍在")
            .last_agent_message
            .as_deref(),
        Some("已完成"),
        "删除不能级联清理任务历史"
    );
    assert!(
        project_store
            .agent_definition_store()
            .list()
            .expect("定义")
            .iter()
            .any(|item| item.id == definition.id),
        "quick definition 不得随手机关卡清除"
    );
    let repeated = fetch_action(cx, &server, &cert, &machine, deletion);
    assert_eq!(
        repeated["result"]["receipt"]["state"], "succeeded",
        "重复删除返回原成功回执"
    );
    assert!(
        app.read_with(cx, |app, _| app.runtime.document.node(retained).is_some()),
        "重试不能误删另一张卡"
    );
    server
        .replace_permissions(
            "phone-a",
            2,
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("control revision=2 降为 read revision=3");
    let retained_snapshot =
        app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let readonly_delete = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"read-delete","workspace_id":id,"route_id":route,"grant_revision":3,
        "command":{"type":"delete_card","expected":retained_snapshot,"confirm_busy":true}
    }});
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, readonly_delete)["error"],
        "unauthorized",
        "只读不能利用删除命令绕过权限"
    );
    assert!(
        app.read_with(cx, |app, _| app.runtime.document.node(retained).is_some()),
        "拒绝只读后保留卡仍在"
    );
    let mut stop = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"read-stop","workspace_id":id,"route_id":route,"grant_revision":3,
        "command":{"type":"interrupt_turn","expected":retained_snapshot}
    }});
    stop["request"]["command"]["expected"]["state"] = json!("thinking");
    stop["request"]["command"]["expected"]["run_id"] = json!("old-run");
    stop["request"]["command"]["expected"]["session_id"] = json!("old-session");
    stop["request"]["command"]["expected"]["turn_id"] = json!("old-turn");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, stop.clone())["error"],
        "unauthorized",
        "只读不得停止任务"
    );
    server
        .replace_permissions(
            "phone-a",
            3,
            &[MobileProjectAccess {
                workspace_id: id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("停止授权 revision=4");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, stop.clone())["error"],
        "grant_changed",
        "旧授权不可停止"
    );
    stop["request"]["grant_revision"] = json!(4);
    let _ = fetch_action(cx, &server, &cert, &machine, stop.clone());
    let stop_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":id,"operation_id":"read-stop"});
    let stopped = wait_receipt(cx, &server, &cert, &machine, stop_query);
    assert_eq!(
        stopped["result"]["receipt"]["state"], "rejected",
        "旧回合中断不能成功"
    );
    assert_eq!(
        stopped["result"]["receipt"]["error"], "card_changed",
        "必须绑定原运行身份"
    );
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, stop.clone())["result"]["receipt"]["state"],
        "rejected",
        "重复请求沿用原拒绝"
    );
    stop["request"]["command"]["expected"]["turn_id"] = json!("new-turn");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, stop)["error"],
        "idempotency_conflict",
        "不可把旧操作转为新回合中断"
    );
    assert!(
        app.read_with(cx, |app, _| app.runtime.document.node(retained).is_some()),
        "停止不得删除卡片"
    );
    server.revoke("phone-a").expect("撤销");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, query)["error"],
        "unauthorized",
        "撤销后连旧回执也不能读取"
    );
}

fn wait_receipt(
    cx: &mut gpui::VisualTestContext,
    server: &MobileServer,
    cert: &[u8],
    machine: &str,
    query: serde_json::Value,
) -> serde_json::Value {
    // 5s = foreground 2s + 360ms save debounce + TLS/SQLite scheduling allowance.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let value = fetch_action(cx, server, cert, machine, query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            return value;
        }
        assert!(std::time::Instant::now() < deadline, "命令回执必须有界完成");
    }
}
