use crate::harness::{drain_until, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use runtime::{AgentTaskState, NodeView};
use std::time::Duration;

/// App 后台派发 Codex 真实首条任务，回执、原生 TUI 和历史必须属于同一卡片。
///
/// 数据构造：隔离测试数据库及临时项目；无工具任务只要求返回固定文本，不修改全局 Hook。
/// 超时：启动 30s，模型 120s，本地历史 RPC 5s；分别覆盖冷启动、生成和本机通信。
/// 执行过程：创建空卡 → App 派发 → 收会话回执 → 读历史 → 关闭 → 将元数据标为旧卡 → 正式重开 → 删除卡片并连续两次读取归档。
/// 预期结果：创建时无进程；真实回复与 Done；重开不增加新回合；删除后两个run的准确AppDB身份均保留，连续读取不受旧连接影响。
/// 这是 headless GPUI + 真实 Codex，不能替代真实 .app / iPhone 验收。
#[test]
#[ignore = "requires installed Codex and real model access"]
fn e2e_mobile_controlled_codex_dispatch() {
    let mut test_cx = TestAppContext::single();
    let socket_dir = tempfile::tempdir().expect("隔离路由");
    test_cx.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket_dir.path().join("norma.sock"), cx)
            .expect("正式路由")
    });
    let (app, cx) = make_app_real(&mut test_cx);
    let directory = tempfile::tempdir().expect("应创建隔离项目");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(directory.path(), window, cx)
        })
    });
    let (id, receipt) = app.update(cx, |app, cx| {
        let id = app
            .runtime
            .create_mobile_agent_card(
                AgentProfile::Codex,
                None,
                None,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("应创建卡片");
        assert_eq!(app.runtime.live_len(), 0, "创建卡片不能派发任务");
        let receipt = app
            .start_mobile_codex(
                id,
                "Do not use any tools or modify files. Reply with exactly NORMA_CODEX_APP_READY."
                    .into(),
                cx,
            )
            .expect("应接受派发");
        (id, receipt)
    });
    let session = drain_until(
        &app,
        cx,
        |runtime| runtime.agent_session_id(id),
        Duration::from_secs(30),
    )
    .expect("后台启动必须准确附着");
    assert_eq!(
        receipt.try_recv().expect("应有回执").expect("应启动成功"),
        session,
        "回执与卡片必须绑定同一会话"
    );
    let screen = drain_until(
        &app,
        cx,
        |runtime| {
            if let Some(
                runtime::NodeRunState::Finished { .. } | runtime::NodeRunState::Error { .. },
            ) = runtime.node_run_state(id)
            {
                let text = runtime
                    .node_view(id)
                    .map(|view| {
                        let NodeView::Term { snapshot, .. } = view;
                        snapshot
                            .rows
                            .iter()
                            .flatten()
                            .map(|cell| cell.c)
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                panic!(
                    "原生终端提前退出；状态：{:?}；界面：{text}",
                    runtime.agent_task_state(id)
                );
            }
            if runtime.agent_task_state(id) != Some(AgentTaskState::Done) {
                return None;
            }
            let NodeView::Term { snapshot, .. } = runtime.node_view(id)?;
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            text.contains("NORMA_CODEX_APP_READY").then_some(text)
        },
        Duration::from_secs(120),
    )
    .expect("真实终端回复和协议完成必须都到达");
    assert!(!screen.contains("no rollout found"), "不能附着不存在的历史");
    let history = app
        .read_with(cx, |app, _| {
            app.runtime.mobile_codex_history(id, &session, None)
        })
        .expect("应读取本次历史")
        .recv_timeout(Duration::from_secs(5))
        .expect("应收到历史")
        .expect("历史 RPC 必须成功");
    assert!(
        history["data"]
            .as_array()
            .expect("历史应包含回合列表")
            .iter()
            .flat_map(|turn| turn["items"].as_array().into_iter().flatten())
            .any(|item| item["type"] == "agentMessage"
                && item["text"]
                    .as_str()
                    .is_some_and(|text| text.trim() == "NORMA_CODEX_APP_READY")),
        "完整历史必须有本次真实消息"
    );
    // Phone reads must use the formal network gateway, not a trusted local runtime helper.
    let store = storage::AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Codex Chat")
        .expect("项目");
    store
        .mobile_access_store()
        .register(
            "phone-chat",
            "iPhone",
            &"a".repeat(64),
            &[storage::MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: storage::MobilePermission::Read,
            }],
        )
        .expect("只读对话权限");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = app::MobileServer::start_with_conversations(
        app::MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx)),
        None,
        app.read_with(cx, |app, cx| app.mobile_conversation_source_for_e2e(cx)),
    )
    .expect("正式历史入口");
    let card = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let index = storage::MobileTurnStore::for_workspace(
        app.read_with(cx, |app, _| app.app_database_path_for_e2e()),
        project.workspace_id.clone(),
    );
    let original_key = storage::MobileSessionKey {
        node_id: id,
        profile: "Codex".into(),
        run_id: card.run_id.clone().expect("原运行"),
        session_id: session.clone(),
    };
    let saved = index
        .session(&original_key)
        .expect("启动索引")
        .expect("正式App启动必须保存身份");
    assert_eq!(
        saved.session.workspace_root.canonicalize().expect("项目根"),
        directory.path().canonicalize().expect("测试项目"),
        "索引必须绑定原项目而非最新历史扫描"
    );
    assert!(
        saved.transcript_path.is_none(),
        "Codex由官方session协议定位，不能猜测Claude路径"
    );
    let route = app.read_with(cx, |app, _| {
        app.runtime
            .agent_hook_config()
            .expect("Hook 路由")
            .workspace_route_id
            .clone()
    });
    let action = serde_json::json!({"type":"conversation","device_id":"phone-chat","token":"a".repeat(64),"request":{
        "workspace_id":project.workspace_id,"route_id":route,"node_id":id,"run_id":card.run_id,"session_id":session,"cursor":null
    }});
    let network_history =
        crate::e2e_mobile_snapshot::fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        network_history["result"]["status"], "conversation",
        "只读手机必须读取正式历史"
    );
    assert!(
        network_history["result"]["conversation"]["page"]["items"]
            .as_array()
            .expect("正文列表")
            .iter()
            .any(|item| item["text"] == "NORMA_CODEX_APP_READY"),
        "WSS 必须传回实际 Provider 回复"
    );
    let mut foreign = action.clone();
    foreign["request"]["session_id"] = serde_json::json!("foreign-thread");
    assert_eq!(
        crate::e2e_mobile_snapshot::fetch_action(cx, &server, &cert, &machine, foreign)["error"],
        "session_changed",
        "不能读取另一会话"
    );
    server.revoke("phone-chat").expect("撤销");
    assert_eq!(
        crate::e2e_mobile_snapshot::fetch_action(cx, &server, &cert, &machine, action)["error"],
        "unauthorized",
        "撤销后不能继续获取历史"
    );
    if let Ok(probe) = std::env::var("NORMA_SWIFT_PROBE") {
        crate::e2e_mobile_conversation::verify_native(
            &app,
            cx,
            directory.path(),
            &probe,
            "conversation",
        );
    }
    app.update(cx, |app, _| {
        assert!(
            app.runtime
                .mobile_codex_history(id, "foreign-thread", None)
                .is_err(),
            "错误会话不得读取"
        );
        app.runtime.kill_all();
        assert!(
            app.runtime
                .mobile_codex_history(id, &session, None)
                .is_err(),
            "关窗后控制连接必须失效"
        );
    });
    // 模拟已保存旧卡的 metadata；复用刚验证过的真实历史，不启动旧的非受控 CLI。
    let receipt = app.update(cx, |app, cx| {
        let canvas::CanvasNodeKind::Agent(spec) =
            &mut app.runtime.document.node_mut(id).expect("原卡").kind
        else {
            panic!("Agent")
        };
        spec.mobile_control = false;
        app.reopen_mobile_agent(id, &session, cx)
            .expect("显式重开应接受")
    });
    let result = drain_until(
        &app,
        cx,
        |_| receipt.try_recv().ok(),
        Duration::from_secs(30),
    )
    .expect("重开应返回回执")
    .expect("旧历史应可恢复");
    assert_eq!(result, session, "重开必须仍是原会话");
    let reopened_key = storage::MobileSessionKey {
        run_id: app.read_with(cx, |app, _| {
            app.runtime.mobile_agent_run_id(id).expect("新运行").into()
        }),
        ..original_key.clone()
    };
    assert_ne!(
        reopened_key.run_id, original_key.run_id,
        "相同会话重新运行必须登记独立身份"
    );
    let resumed = app
        .read_with(cx, |app, _| {
            app.runtime.mobile_codex_history(id, &session, None)
        })
        .expect("已重连历史")
        .recv_timeout(Duration::from_secs(5))
        .expect("本地 RPC")
        .expect("历史");
    assert_eq!(
        resumed["data"], history["data"],
        "重开不能新增假任务或改变已有回合"
    );
    app.update(cx, |app, _| {
        assert_ne!(
            app.runtime.agent_task_state(id),
            Some(AgentTaskState::Thinking),
            "重开不能派发新任务"
        );
        assert!(app.runtime.node_view(id).is_some(), "原生终端应重新附着");
        app.runtime.kill_all();
        app.runtime.remove_node(id);
        assert!(
            app.runtime.document.node(id).is_none(),
            "删除后画布不得保留卡片"
        );
    });
    assert!(
        index.session(&original_key).expect("旧身份").is_some(),
        "删除不能清理首次启动的历史入口"
    );
    assert!(
        index.session(&reopened_key).expect("重开身份").is_some(),
        "删除不能清理重开后的历史入口"
    );
    store
        .mobile_access_store()
        .register(
            "phone-archive",
            "Archive iPhone",
            &"b".repeat(64),
            &[storage::MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: storage::MobilePermission::Read,
            }],
        )
        .expect("另一手机只读归档授权");
    let archived = serde_json::json!({"type":"archived_conversation","device_id":"phone-archive","token":"b".repeat(64),"profile":"Codex",
        "request":{"workspace_id":project.workspace_id,"route_id":app.read_with(cx, |app, _| app.norma_workspace_identity_for_e2e().expect("当前路由").1),
        "node_id":id,"run_id":original_key.run_id,"session_id":session}});
    let result =
        crate::e2e_mobile_snapshot::fetch_action(cx, &server, &cert, &machine, archived.clone());
    assert_eq!(
        result["result"]["status"], "conversation",
        "原host关闭并删卡后必须能只读原始历史: {result}"
    );
    assert_eq!(
        result["result"]["conversation"]["page"]["items"],
        network_history["result"]["conversation"]["page"]["items"],
        "只读归档不能增加回合或丢失完整正文"
    );
    // 2次读取 = 首次归档读取 + 同一卡片再次打开；第二次守住断线缓存回归。
    let repeated = crate::e2e_mobile_snapshot::fetch_action(cx, &server, &cert, &machine, archived);
    assert!(
        repeated.get("error").is_none(),
        "再次打开同一卡片不应返回 history_unavailable: {repeated}"
    );
    assert_eq!(
        repeated["result"]["conversation"]["page"]["items"],
        network_history["result"]["conversation"]["page"]["items"],
        "再次读取必须保留同一份完整 Provider 历史"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().len()),
        0,
        "归档读取不能重建卡片"
    );
}
