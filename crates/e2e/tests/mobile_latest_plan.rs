use app::{LiveCommandRequest, MobileAccess, MobileCommand, MobileServer};
use norma::mobile::ServerIdentity;
use serde_json::{json, Value};
use std::time::Duration;
use storage::{AppStore, MobilePermission, MobileProjectAccess, ProjectPlanStore};
#[path = "support/mobile.rs"]
mod support;

/// 最新计划订阅从空态发现其他手机发布的新方向，并保留按ID订阅的历史计划。
///
/// 数据构造：同项目reader只读、writer可控制；未授权private项目预置secret计划；模型fixture只保存建议不启动Agent。
/// 执行过程：空基线/无正文心跳 → writer经正式命令发布plan-a → 固定订阅a → writer发布b → 核对两个流 → 撤销reader。
/// 预期结果：最新流自动a→b，固定流仍为a；不泄露private内容，空态无虚构待办，撤销后不继续读。
/// 变更读超时1s=50ms合并+950ms调度余量，低于2s心跳，避免漏掉信号被轮询掩盖。
#[test]
fn latest_plan_stream_discovers_cross_client_publications_without_polling() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "计划")
        .expect("项目")
        .workspace_id;
    let plans = ProjectPlanStore::new(store.path().into());
    let draft = agent::ProjectPlanDraft {
        summary: "真实存储的简报".into(),
        todos: vec![],
    };
    let private_path = dir.path().join("private");
    std::fs::create_dir(&private_path).expect("未授权项目目录");
    let private = store
        .workspace_identity_store()
        .resolve_or_create(&private_path, "未授权")
        .expect("独立项目身份")
        .workspace_id;
    plans
        .publish("secret", &private, "route-private", "不可见方向", &draft)
        .expect("未授权计划");
    for (device, permission) in [
        ("reader", MobilePermission::Read),
        ("writer", MobilePermission::Control),
    ] {
        store
            .mobile_access_store()
            .register(
                device,
                device,
                &"a".repeat(64),
                &[MobileProjectAccess {
                    workspace_id: workspace.clone(),
                    access: permission,
                }],
            )
            .expect("仅授权当前项目");
    }
    // Two planned calls are the only fixture work; completion runs the production change notification.
    let (commands, calls) = async_channel::bounded::<LiveCommandRequest>(2);
    let publisher = std::thread::spawn(move || {
        while let Ok(call) = calls.recv_blocking() {
            assert!(call.lease.begin(), "必须保留正式授权租约");
            let MobileCommand::GeneratePlan { direction, .. } = &call.request.command else {
                panic!("只允许生成建议");
            };
            let plan = plans
                .publish(
                    &call.request.operation_id,
                    &call.request.workspace_id,
                    &call.request.route_id,
                    direction,
                    &draft,
                )
                .expect("后台持久化新计划");
            call.reply
                .send(Ok(json!({"plan_id":plan.plan_id,"status":"proposed"})))
                .expect("真实命令完成信号");
        }
    });
    let identity = ServerIdentity::generate().expect("TLS身份");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        None,
        Some(commands),
    )
    .expect("正式WSS");
    let mut watch = support::connect(server.addresses()[0], cert.clone());
    let latest = json!({"type":"watch_plan","device_id":"reader","token":"a".repeat(64),"workspace_id":workspace});
    let initial = support::exchange(&mut watch, &machine, "latest", latest.clone());
    assert_eq!(
        initial["result"]["status"], "plan_event",
        "未指定ID应订阅当前项目最新计划"
    );
    assert!(
        initial["result"]["plan_event"]["snapshot"]["plan"].is_null(),
        "没有计划不能借用其他项目"
    );
    assert_eq!(
        initial["result"]["plan_event"]["snapshot"]["decisions"],
        json!([]),
        "空计划无待办决定"
    );
    assert!(
        !initial.to_string().contains("不可见方向"),
        "未授权正文不可泄露"
    );
    let stream = initial["result"]["plan_event"]["stream_id"].clone();
    let heartbeat: Value =
        serde_json::from_str(watch.read().expect("空态保活").to_text().expect("文本"))
            .expect("JSON");
    assert!(
        heartbeat["result"]["plan_event"]["snapshot"].is_null(),
        "空态保活也不能重复正文"
    );
    watch
        .get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("小于心跳的事件预算");
    let mut writer = support::connect(server.addresses()[0], cert.clone());
    let publish = |id: &str, direction: &str| json!({"type":"command","device_id":"writer","token":"a".repeat(64),"request":{"operation_id":id,"workspace_id":workspace,"route_id":"route-a","grant_revision":1,"command":{"type":"generate_plan","direction":direction}}});
    let sent = support::exchange(
        &mut writer,
        &machine,
        "a",
        publish("plan-a", "先解决连接延迟"),
    );
    assert_eq!(
        sent["result"]["status"], "receipt",
        "另一手机应走持久化命令"
    );
    let first: Value = serde_json::from_str(
        watch
            .read()
            .expect("不等心跳收到首个计划")
            .to_text()
            .expect("文本"),
    )
    .expect("JSON");
    assert_eq!(
        first["result"]["plan_event"]["sequence"], 1,
        "空态到计划推进一次"
    );
    assert_eq!(
        first["result"]["plan_event"]["snapshot"]["plan"]["plan_id"], "plan-a",
        "必须发现准确首个计划"
    );
    let mut fixed = support::connect(server.addresses()[0], cert.clone());
    let mut action = latest;
    action["plan_id"] = json!("plan-a");
    let fixed_first = support::exchange(&mut fixed, &machine, "fixed", action);
    assert_eq!(
        fixed_first["result"]["plan_event"]["snapshot"]["plan"]["plan_id"], "plan-a",
        "旧API固定订阅保持兼容"
    );
    support::exchange(
        &mut writer,
        &machine,
        "b",
        publish("plan-b", "再验证同会话续发"),
    );
    let next: Value = serde_json::from_str(
        watch
            .read()
            .expect("第二次方向立即更新")
            .to_text()
            .expect("文本"),
    )
    .expect("JSON");
    let event = &next["result"]["plan_event"];
    assert_eq!(event["stream_id"], stream, "不能靠重连发现新计划");
    assert_eq!(event["sequence"], 2, "只有真实变更增加序号");
    assert_eq!(
        event["snapshot"]["plan"]["direction"], "再验证同会话续发",
        "必须切到新方向"
    );
    assert_ne!(
        event["snapshot"]["plan"]["plan_id"], "plan-a",
        "旧计划不能继续冒充最新"
    );
    let fixed_next: Value =
        serde_json::from_str(fixed.read().expect("旧计划保活").to_text().expect("文本"))
            .expect("JSON");
    assert_eq!(
        fixed_next["result"]["plan_event"]["sequence"], 0,
        "固定计划不能被新计划替换"
    );
    assert!(
        fixed_next["result"]["plan_event"]["snapshot"].is_null(),
        "固定计划无变化不重发正文"
    );
    server.revoke("reader").expect("撤销只读手机");
    assert!(watch.read().is_err(), "撤销后不能继续接收计划");
    server.shutdown();
    drop(server);
    publisher.join().expect("回收fixture");
}
