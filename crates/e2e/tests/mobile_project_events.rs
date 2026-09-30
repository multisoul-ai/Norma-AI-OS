use app::{LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{CardSnapshot, ProjectSnapshot, ServerIdentity};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 真实 WSS 项目流绑定授权与新连接基线，卡片变化/删除增量、换窗口全量、撤销关闭。
///
/// 数据构造：隔离SQLite、手机A只读项目A；两张空卡，前台fixture返回内存快照，socket超时3s。
/// 执行过程：订阅 → 首个快照 → 改卡2删卡1 → delta → 仅耗时变化增量 → route替换 → snapshot → 改权限关闭 → 新流 → 越权/关服。
/// 预期结果：基线/序号/来源明确；无伪造卡1，无跨项目内容；新连接不续用旧流，撤销即关闭。
#[test]
fn project_stream_updates_and_reconnects_without_reusing_old_baseline() {
    let directory = tempfile::tempdir().expect("隔离项目");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "A")
        .expect("项目")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "Phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("测试只读授权");
    let card = |node_id| CardSnapshot {
        node_id,
        profile: "Codex".into(),
        title: format!("Card {node_id}"),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "idle".into(),
        run_id: None,
        session_id: None,
        turn_id: None,
        task_title: None,
    };
    let live = Arc::new(Mutex::new(ProjectSnapshot {
        workspace_id: workspace.clone(),
        display_name: "A".into(),
        state: "open".into(),
        route_id: Some("route-a".into()),
        card_count: Some(2),
        cards: Some(vec![card(1), card(2)]),
    }));
    let (tx, rx) = async_channel::bounded::<LiveSnapshotRequest>(8); // eight is the shared connection limit.
    let state = live.clone();
    let frontend = std::thread::spawn(move || {
        while let Ok(request) = rx.recv_blocking() {
            let _ = request
                .reply
                .send(vec![state.lock().expect("fixture").clone()]);
        }
    });
    let identity = ServerIdentity::generate().expect("TLS");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(tx),
    )
    .expect("服务");
    let watch = json!({"type":"watch_project","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace});
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let first = support::exchange(&mut socket, &machine, "watch-a", watch.clone());
    assert_eq!(
        first["result"]["status"], "project_event",
        "订阅必须得到正式事件"
    );
    let first = &first["result"]["event"];
    assert_eq!(first["sequence"], 0, "连接从全量基线开始");
    assert_eq!(first["kind"], "snapshot", "第一项不能为无法应用的delta");
    assert_eq!(first["project"]["cards"][0]["node_id"], 1, "初始卡1存在");
    let stream_id = first["stream_id"].clone();
    {
        let mut project = live.lock().expect("fixture");
        let cards = project.cards.as_mut().expect("卡片");
        cards.remove(0);
        cards[0].state = "ready".into();
        project.card_count = Some(1);
    }
    let next: Value = serde_json::from_str(
        socket
            .read()
            .expect("真实变化应在2秒保活核验内到达")
            .to_text()
            .expect("JSON"),
    )
    .expect("解析");
    let delta = &next["result"]["event"];
    assert_eq!(delta["stream_id"], stream_id, "同连接来源不改变");
    assert_eq!(delta["sequence"], 1, "一次变化推进一次");
    assert_eq!(delta["upserted"][0]["node_id"], 2, "只发送变化卡2");
    assert_eq!(delta["removed"], json!([1]), "明确删除卡1");
    assert_eq!(delta["order"], json!([2]), "卡1不残留在显示顺序");
    assert!(
        delta.get("project").is_none(),
        "同窗口不能伪装为重复完整快照"
    );
    live.lock().expect("fixture").cards.as_mut().expect("卡片")[0].task_timing =
        Some(norma::mobile::TaskTiming {
            started_at_unix_ms: 1000,
            completed_elapsed_ms: Some(60 * 1000), // one minute; display metadata alone changes
        });
    let timing: Value =
        serde_json::from_str(socket.read().expect("计时增量").to_text().expect("JSON"))
            .expect("解析");
    let timing = &timing["result"]["event"];
    assert_eq!(
        timing["kind"], "delta",
        "仅计时变化也应送达，不能被目标相等比较吞掉"
    );
    assert_eq!(
        timing["upserted"][0]["task_timing"]["completed_elapsed_ms"], 60000,
        "须保留准确的Mac耗时"
    );
    assert_eq!(
        timing["upserted"].as_array().expect("增量").len(),
        1,
        "只能发送变化卡2"
    );
    assert!(timing.get("project").is_none(), "不能退回重复全量项目传输");
    live.lock().expect("fixture").route_id = Some("route-b".into());
    let next: Value = serde_json::from_str(socket.read().expect("新窗口").to_text().expect("JSON"))
        .expect("解析");
    assert_eq!(
        next["result"]["event"]["kind"], "snapshot",
        "窗口变化必须重置卡片基线"
    );
    assert_eq!(
        next["result"]["event"]["project"]["route_id"], "route-b",
        "不可续用旧窗口身份"
    );
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("更新授权");
    assert!(socket.read().is_err(), "权限变化必须关闭此前流");
    let mut fresh = support::connect(server.addresses()[0], cert.clone());
    let reset = support::exchange(&mut fresh, &machine, "watch-b", watch);
    assert_eq!(
        reset["result"]["event"]["sequence"], 0,
        "新连接必须重新建立基线"
    );
    assert_ne!(
        reset["result"]["event"]["stream_id"], stream_id,
        "旧游标不能延续新连接"
    );
    assert_eq!(
        reset["result"]["grant"]["revision"], 2,
        "新流必须使用最新权限"
    );
    let mut denied = support::connect(server.addresses()[0], cert);
    let refusal = support::exchange(
        &mut denied,
        &machine,
        "watch-hidden",
        json!({"type":"watch_project","device_id":"phone-a","token":"a".repeat(64),"workspace_id":"hidden"}),
    );
    assert_eq!(refusal["error"], "unauthorized", "越权不能订阅");
    assert!(refusal.get("result").is_none(), "不能泄漏未授权项目");
    server.shutdown();
    assert!(fresh.read().is_err(), "服务关闭必须终止流");
    drop(denied);
    drop(fresh);
    drop(socket);
    drop(server);
    frontend.join().expect("前台请求任务结束");
}
