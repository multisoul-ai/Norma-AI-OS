use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app},
};
use app::{MobileAccess, MobileServer, MobileSources};
use gpui::TestAppContext;
use serde_json::json;
use storage::{
    AppStore, MobilePermission, MobileProjectAccess, MobileSession, MobileSessionKey,
    MobileTurnStore,
};

/// 删除卡片后，项目归档仍列出准确会话；只读授权不能扩大到其他项目或过期窗口。
///
/// 数据构造：隔离项目A的一张空卡和一份明确标注的会话身份fixture；项目B无授权，无Provider任务。
/// 执行过程：正式窗口登记A → 保存身份 → 删除卡片 → WSS查归档 → 越权查询 → 切到B后重放旧route。
/// 预期结果：只保留A的原始run/session和标题，不暴露磁盘路径；越权与旧route无结果，读取不启动Agent。
#[test]
fn e2e_mobile_archive_survives_card_deletion_and_checks_scope() {
    check_archive(None);
}

/// 原生客户端通过正式WSS、SQLite索引与Claude原文件读取已删卡的会话，跨分片保持完整消息。
///
/// 数据构造：第一条7000字，第二条200000汉字×3=600000字节，合计621000正文字节超过512KiB单帧；独立文件fixture，不运行模型。
/// 执行过程：登记/删卡 → 原生客户端只读配对 → 枚举会话 → 完整分片读取 → 错会话与实时旧卡拒绝。
/// 预期结果：两条原始正文逐字相等、无卡片、无新任务，完整消息不得被摘要替代。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn e2e_mobile_archive_native_transport() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("原生probe");
    check_archive(Some(&probe));
}

fn check_archive(probe: Option<&str>) {
    let mut test = TestAppContext::single();
    let socket = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket.path().join("norma.sock"), cx).expect("正式路由")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let root = tempfile::tempdir().expect("A");
    let other = tempfile::tempdir().expect("B");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(root.path(), window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Claude,
                None,
                None,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("空卡")
    });
    let (database, workspace) = app.read_with(cx, |app, _| {
        (
            app.app_database_path_for_e2e(),
            app.norma_workspace_identity_for_e2e().expect("身份").0,
        )
    });
    let store = AppStore::new(database.clone());
    let key = MobileSessionKey {
        node_id: node,
        profile: "Claude".into(),
        run_id: "archive-run".into(),
        session_id: "archive-session".into(),
    };
    let turns = MobileTurnStore::for_workspace(database.clone(), workspace.clone());
    turns
        .record_session(&MobileSession {
            key: key.clone(),
            workspace_root: root.path().into(),
            title: "修复登录".into(),
        })
        .expect("实际SQLite身份");
    app.update(cx, |app, _| {
        assert!(app.runtime.remove_node(node), "卡片必须已删除才验证归档")
    });
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            terminals: app.read_with(cx, |app, cx| app.mobile_terminal_source_for_e2e(cx)),
            snapshots: app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx)),
            conversations: app.read_with(cx, |app, cx| app.mobile_conversation_source_for_e2e(cx)),
            ..Default::default()
        },
    )
    .expect("正式服务");
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace}),
    );
    let scope =
        json!({"workspace_id":workspace,"route_id":snapshot["result"]["projects"][0]["route_id"]});
    let action = json!({"type":"session_archive","device_id":"phone-a","token":"a".repeat(64),"request":scope});
    let result = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        result["result"]["status"], "session_archive",
        "删除后必须能从项目查到会话: {result}"
    );
    let items = result["result"]["archive"]["items"]
        .as_array()
        .expect("归档列表");
    assert_eq!(items.len(), 1, "只有已登记的会话，不能补造其他历史");
    assert_eq!(
        items[0]["key"],
        json!(key),
        "保持准确的Provider/run/session身份"
    );
    assert_eq!(items[0]["title"], "修复登录", "删除后保留启动标题");
    assert!(
        items[0].get("workspace_root").is_none(),
        "元数据不暴露磁盘根路径"
    );
    assert!(
        items[0].get("transcript_path").is_none(),
        "手机不得接收文件引用"
    );
    assert!(
        result["result"]["archive"]["next_cursor"].is_null(),
        "一份身份不应伪造下一页"
    );
    // 受控文件位于当前Provider配置根下的独立临时目录；不改变HOME或用户已有会话。
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("Home")).join(".claude")
        });
    let projects = config.join("projects");
    std::fs::create_dir_all(&projects).expect("Provider目录");
    let transcript = tempfile::Builder::new()
        .prefix("norma-archive-test-")
        .tempdir_in(projects)
        .expect("独立会话fixture");
    let path = transcript.path().join("archive-session.jsonl");
    let body = "完整消息".repeat(1750); // 4×1750=7000汉字，明显超过旧摘要上限1200字符。
    std::fs::write(&path, format!("{}\n", json!({"type":"assistant","uuid":"reply-a","sessionId":"archive-session","cwd":root.path(),"message":{"role":"assistant","content":[{"type":"text","text":body}]}}))).expect("原始文件");
    let writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(database).with_turn_store(turns),
    )
    .expect("正式writer");
    // 5s沿用后台SQLite提交预算；入队不代表已保存。
    writer
        .bind_session_transcript(key.clone(), path.clone())
        .expect("路径入队")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("回执")
        .expect("路径提交");
    let read = json!({"type":"archived_conversation","device_id":"phone-a","token":"a".repeat(64),"profile":"Claude",
        "request":{"workspace_id":workspace,"route_id":scope["route_id"],"node_id":node,"run_id":"archive-run","session_id":"archive-session"}});
    let page = fetch_action(cx, &server, &cert, &machine, read.clone());
    assert_eq!(
        page["result"]["status"], "conversation",
        "删除后必须读取原始正文: {page}"
    );
    assert_eq!(
        page["result"]["conversation"]["page"]["items"][0]["text"], body,
        "完整7000字不裁剪"
    );
    assert_eq!(
        page["result"]["conversation"]["page"]["items"][0]["id"], "reply-a",
        "保持Provider消息身份"
    );
    let mut wrong = read.clone();
    wrong["request"]["session_id"] = json!("another-session");
    let denied = fetch_action(cx, &server, &cert, &machine, wrong);
    assert_eq!(
        denied["error"], "history_unavailable",
        "身份不符不能寻找最新文件"
    );
    assert!(denied.get("result").is_none(), "错误会话不得收到正文");
    let mut live = read.clone();
    live["type"] = json!("conversation");
    live.as_object_mut().expect("请求").remove("profile");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, live)["error"],
        "session_changed",
        "归档不能放松实时会话校验"
    );
    if let Some(probe) = probe {
        use std::io::Write;
        let large = "归".repeat(200_000); // 600000字节，触发真实WSS分片协议。
        writeln!(std::fs::OpenOptions::new().append(true).open(&path).expect("追加原文件"), "{}", json!({"type":"assistant","uuid":"reply-b","sessionId":"archive-session","cwd":root.path(),"message":{"role":"assistant","content":[{"type":"text","text":large}]}})).expect("第二条完整消息");
        crate::e2e_mobile_conversation::verify_native(&app, cx, root.path(), probe, "archive");
    }
    std::fs::remove_file(path).expect("模拟Provider清理原文件");
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, read)["error"],
        "history_unavailable",
        "源文件丢失须明确失败，不能回退到摘要或其他会话"
    );
    let mut foreign = action.clone();
    foreign["request"]["workspace_id"] = json!("ungranted");
    let denied = fetch_action(cx, &server, &cert, &machine, foreign);
    assert_eq!(denied["error"], "unauthorized", "未授权项目必须拒绝");
    assert!(denied.get("result").is_none(), "越权不能泄漏列表");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(other.path(), window, cx)
        })
    });
    let stale = fetch_action(cx, &server, &cert, &machine, action);
    assert_eq!(stale["error"], "route_expired", "关闭原项目后旧route失效");
    assert!(stale.get("result").is_none(), "过期窗口不能导出会话");
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "归档浏览不能启动Agent"
    );
}
