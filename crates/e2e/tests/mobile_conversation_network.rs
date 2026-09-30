use app::{LiveConversationRequest, MobileAccess, MobileServer};
use norma::mobile::ConversationPage;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod support;

/// 只读手机可读取当前会话，但历史读取期间撤销权限必须阻止内容返回。
///
/// 数据构造：隔离项目 A / 未授权 B、phone-a 的 read 权限、有界历史通道容量8与正式连接上限一致。
/// 执行过程：A 请求通过并读取一条消息 → B 越权被拒且不进前台 → A 第二次请求被截留 → 撤销 → 完成读取。
/// 预期结果：只读成功得到正文；B 不泄漏；撤销竞争时返回 unauthorized，不回传已读正文。
#[test]
fn conversation_read_rechecks_permission_after_background_work() {
    let dir = tempfile::tempdir().expect("隔离库");
    let store = AppStore::new(dir.path().join("app.db"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "A")
        .expect("项目");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (tx, rx) = async_channel::bounded::<LiveConversationRequest>(8);
    let server = Arc::new(
        MobileServer::start_with_conversations(
            MobileAccess::with_identity_for_e2e(store, identity),
            vec!["127.0.0.1:0".parse().expect("回环")],
            None,
            None,
            Some(tx),
        )
        .expect("服务"),
    );
    let action = json!({"type":"conversation","device_id":"phone-a","token":"a".repeat(64),"request":{
        "workspace_id":project.workspace_id,"route_id":"route-a","node_id":7,"run_id":"run-a","session_id":"session-a","cursor":null
    }});
    let fetch = |action: serde_json::Value| {
        let server = server.clone();
        let cert = cert.clone();
        let machine = machine.clone();
        std::thread::spawn(move || {
            let mut socket = support::connect(server.addresses()[0], cert);
            support::exchange(&mut socket, &machine, "read", action)
        })
    };
    let pending = || {
        // 5s = 3s TLS + 2s thread scheduling; test failures must not leave an unbounded receive.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(request) = rx.try_recv() {
                break request;
            }
            assert!(Instant::now() < deadline, "授权请求必须到达前台");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    let page = || {
        ConversationPage::from_codex(json!({"data":[{"id":"turn-a","items":[{"id":"reply-a","type":"agentMessage","text":"真实读取正文"}]}],"nextCursor":null})).expect("正文投影")
    };
    let first = fetch(action.clone());
    pending().reply.send(Ok(page())).expect("模拟后台完成");
    let result = first.join().expect("读取完成");
    assert_eq!(
        result["result"]["conversation"]["page"]["items"][0]["text"], "真实读取正文",
        "只读权限允许历史查询"
    );
    let mut foreign = action.clone();
    foreign["request"]["workspace_id"] = json!("project-b");
    assert_eq!(
        fetch(foreign).join().expect("越权完成")["error"],
        "unauthorized",
        "B 未授权必须拒绝"
    );
    assert!(rx.try_recv().is_err(), "未授权项目不得进入前台读取");
    let second = fetch(action);
    let held = pending();
    server.revoke("phone-a").expect("读取期间撤销");
    held.reply.send(Ok(page())).expect("迟到正文");
    let denied = second.join().expect("回查授权后完成");
    assert_eq!(
        denied["error"], "unauthorized",
        "第二次授权校验必须识别撤销"
    );
    assert!(denied.get("result").is_none(), "撤销后不得泄露已读取的正文");
}
