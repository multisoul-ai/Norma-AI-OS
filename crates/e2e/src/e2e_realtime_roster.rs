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
