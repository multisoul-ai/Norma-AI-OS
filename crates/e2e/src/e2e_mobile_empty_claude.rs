use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app},
};
use app::{MobileAccess, MobileServer, MobileSources};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 手机通过实际WSS/项目路由订阅没有聊天文件的Claude，并能发送首条消息。
///
/// 数据构造：临时项目、隔离AppDB、原生echo PTY；Down/Enter模拟启动确认，Hook只绑定session。
///   80×24为默认网格，640×400为卡片尺寸；测试凭据仅用于回环WSS。
/// 执行过程：原卡启动确认 → SessionStart/Ready → WSS订阅空对话 → 校验可发送 → 真实草稿后再次校验。
/// 预期结果：首帧为合法空页且无无限分页；不返回历史错误，不覆盖实际桌面草稿，不创建第二PTY。
#[test]
fn e2e_mobile_empty_claude_watch_and_first_send() {
    let mut owner = TestAppContext::single();
    let directory = tempfile::tempdir().expect("隔离项目");
    owner.update(|cx| {
        app::install_norma_endpoint_for_e2e(directory.path().join("norma.sock"), cx)
            .expect("真实路由")
    });
    let (app, cx) = make_app(&mut owner, echo_factory());
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(directory.path(), window, cx)
        })
    });
    let (node, card, workspace, route, database, sources) = app.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_agent(
                AgentProfile::Claude,
                CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("原生终端");
        for key in [terminal::Key::Down, terminal::Key::Enter] {
            app.runtime
                .send_node_input(
                    node,
                    runtime::NodeInput::Key(terminal::KeyInput {
                        key,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .expect("启动菜单确认");
        }
        let event = norma::agent_hook::parse_hook_event(
            norma::agent_hook::HookSource::Claude,
            node,
            json!({"hook_event_name":"SessionStart", "session_id":"empty-session"}),
        )
        .expect("启动Hook");
        assert!(
            app.runtime.observe_mobile_hook(&event),
            "同一原生运行应接收启动Hook"
        );
        app.runtime
            .apply_agent_task_update_with_outcome(runtime::AgentTaskUpdate {
                node_id: node,
                profile: AgentProfile::Claude,
                provider_event: "SessionStart".into(),
                to: runtime::AgentTaskState::Ready,
                task_title: None,
                session_id: Some("empty-session".into()),
            });
        let (workspace, route) = app.norma_workspace_identity_for_e2e().expect("项目身份");
        (
            node,
            app.runtime.mobile_card_snapshots().remove(0),
            workspace,
            route,
            app.app_database_path_for_e2e(),
            MobileSources {
                conversations: app.mobile_conversation_source_for_e2e(cx),
                snapshots: app.mobile_snapshot_source_for_e2e(cx),
                ..MobileSources::default()
            },
        )
    });
    let store = AppStore::new(database);
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("测试手机授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        sources,
    )
    .expect("正式WSS服务");
    let response = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({
            "type":"watch_conversation", "device_id":"phone-a", "token":"a".repeat(64),
            "request":{"workspace_id":workspace, "route_id":route, "node_id":node,
                       "run_id":card.run_id, "session_id":card.session_id, "cursor":null}
        }),
    );
    assert_eq!(
        response["result"]["status"], "conversation_event",
        "没有聊天文件也必须结束首屏加载: {response}"
    );
    assert!(response["error"].is_null(), "空会话不能触发历史错误重连");
    let page = &response["result"]["conversation_event"]["page"];
    assert_eq!(page["items"], json!([]), "尚未开始对话时正文必须为空");
    assert_eq!(page["has_more"], false, "空会话不能继续无限分页");
    assert!(page["cursor"].is_string(), "Claude增量订阅必须保留合法游标");
    app.update(cx, |app, _| {
        assert_eq!(
            app.runtime.validate_mobile_message(&card, "你好"),
            Ok(()),
            "首次发送不能被启动菜单拦截"
        );
        app.runtime
            .send_node_input(node, runtime::NodeInput::Text("桌面草稿".into()))
            .expect("实际输入");
        assert_eq!(
            app.runtime.validate_mobile_message(&card, "手机消息"),
            Err("desktop_input_pending"),
            "实际草稿仍须保护"
        );
        assert_eq!(app.runtime.live_len(), 1, "查看空对话不能另启一个会话");
        app.runtime.kill_all();
    });
}
