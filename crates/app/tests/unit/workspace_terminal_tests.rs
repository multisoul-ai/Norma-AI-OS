use super::{decode, workspace_credentials};
use serde_json::json;

/// 场景：只读富终端经过局域网和中继共用的空间认证入口，不能成为旧终端写动作。
/// 数据构造：1个完整desktop空间目标，run/session绑定原卡；不包含任何输入文本或控制租约。
/// 执行过程：解码新RPC → 获取共用凭据 → 加入未经允许的input字段 → 再解码。
/// 预期结果：读请求进入空间入口；额外输入字段被拒绝；未知动作不被接收。
#[test]
fn workspace_terminal_read_uses_shared_authentication_and_rejects_input() {
    let mut wire = json!({"version":1,"request_id":"read-a","machine_id":"mac-a",
        "action":{"type":"workspace_terminal","device_id":"phone-a","token":"fixture-only",
        "request":{"desktop_workspace_id":"space-a","target":{"area_id":"area-a",
        "definition_id":1,"node_id":2,"run_id":"run-a","session_id":"session-a","task_ref":null},
        "known_version":null,"before":null,"history_version":null,"snapshot_id":null,"offset":null}}});
    let decoded = decode(&wire.to_string()).expect("只读终端必须能解码");
    assert_eq!(
        workspace_credentials(&decoded.action),
        Some(("phone-a", "fixture-only")),
        "LAN和relay都应复用空间凭据校验"
    );
    wire["action"]["input"] = json!("dangerous text");
    assert!(
        decode(&wire.to_string()).is_err(),
        "只读接口不得接收终端输入字段"
    );
}

/// 场景：只读授权能读富终端但不能派工；捕获期间撤销授权也不能泄漏最后一帧。
/// 数据构造：隔离空间/项目各1个，手机仅Read权限；后台捕获只返回1行×1格。
/// 执行过程：读取首帧 → 用同凭据执行任务 → 在下一次捕获时撤销设备 → 检查返回。
/// 预期结果：读请求成功；派工被拒绝；撤权捕获不会回传任何终端内容。
#[test]
fn workspace_terminal_read_grant_and_capture_time_revoke_are_enforced() {
    use crate::mobile::workspace_chat::LiveWorkspaceAction;
    use crate::mobile::{MobileAccess, MobileServer, MobileSources};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = storage::AppStore::initialize(&storage::DatabasePaths::isolated_test(dir.path()))
        .expect("隔离DB");
    let workspace = store
        .desktop_workspace_store()
        .create("验收空间")
        .expect("空间");
    let project = dir.path().join("project");
    std::fs::create_dir(&project).expect("项目目录");
    store
        .desktop_workspace_store()
        .add_project(&workspace.id, &project)
        .expect("区域");
    let area = store
        .desktop_workspace_store()
        .list_projects(&workspace.id)
        .expect("区域")
        .remove(0);
    store
        .mobile_access_store()
        .register_all_projects(
            "reader",
            "只读手机",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            storage::MobilePermission::Read,
        )
        .expect("读授权");
    let access = MobileAccess::open(store.clone()).expect("身份");
    let mac = access.machine_id();
    let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveWorkspaceRequest>(2);
    let server = MobileServer::start_with_mobile_sources(
        access,
        vec!["127.0.0.1:0".parse().expect("回环地址")],
        MobileSources {
            workspaces: Some(sender),
            ..Default::default()
        },
    )
    .expect("只读服务");
    let revoke = Arc::new(AtomicBool::new(false));
    let worker_revoke = revoke.clone();
    let worker = std::thread::spawn(move || {
        // Exactly two captures: successful read and capture-time revoke. Writes never reach this channel.
        for _ in 0..2 {
            let call = receiver.recv_blocking().expect("捕获请求");
            assert!(
                matches!(call.action, LiveWorkspaceAction::Terminal { .. }),
                "写请求不得进入终端捕获入口"
            );
            if worker_revoke.load(Ordering::Acquire) {
                store
                    .mobile_access_store()
                    .revoke("reader")
                    .expect("撤销授权");
            }
            call.reply.send(Ok(json!({"version":"version-a","cols":1,"screen_lines":1,"start_line":0,"total_lines":1,
                "running":true,"alt_screen":false,"cursor":null,"background":0,"foreground":16777215,
                "rows":[[{"text":"A","fg":16777215,"bg":0,"flags":0}]]}))).expect("回复");
        }
    });
    let request = json!({"desktop_workspace_id":workspace.id,"target":{"area_id":area.id,"definition_id":1,"node_id":2,
        "run_id":"run-a","session_id":"session-a","task_ref":null},"known_version":null,"before":null,
        "history_version":null,"snapshot_id":null,"offset":null});
    let read: super::Action = serde_json::from_value(json!({"type":"workspace_terminal","device_id":"reader","token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","request":request})).expect("读动作");
    let first = crate::mobile::workspace_chat::handle(
        &server.shared,
        &mac,
        "reader",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        &read,
    )
    .expect("只读授权可看终端");
    assert_eq!(
        first["status"], "workspace_terminal",
        "Read权限必须能查看真实捕获"
    );
    let write: super::Action = serde_json::from_value(json!({"type":"workspace_message","device_id":"reader","token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "message":{"desktop_workspace_id":workspace.id,"operation_id":"write-a","message_id":"message-a","text":"禁止派工","intent":"execute","target":request["target"],"idea_id":null}})).expect("写动作");
    assert_eq!(
        crate::mobile::workspace_chat::handle(
            &server.shared,
            &mac,
            "reader",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &write
        )
        .expect_err("Read不能派工"),
        "unauthorized",
        "终端读取不能隐式提升到Control"
    );
    revoke.store(true, Ordering::Release);
    assert_eq!(
        crate::mobile::workspace_chat::handle(
            &server.shared,
            &mac,
            "reader",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &read
        )
        .expect_err("撤权后不能回帧"),
        "unauthorized",
        "捕获期间撤权必须挡住最后画面"
    );
    worker.join().expect("后台捕获完成");
}
