use app::{LiveConversationRequest, LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{ConversationPage, ServerIdentity};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 同一WSS推送正文变化和无正文心跳，授权撤销终止准确会话订阅。
///
/// 数据构造：隔离数据库、只读手机、run-a/session-a；前台fixture将“中”改为“中文”。
/// 执行过程：订阅首屏 → 修改页 → 收到sequence1 → 心跳sequence1 → 撤销。
/// 预期结果：变化不会重复ID；心跳无完整页；撤销关闭，错误项目不能泄露内容。
#[test]
fn conversation_watch_sends_changes_and_revokes_live_stream() {
    check(None, false);
}

/// 无关设备反复触发全局变更时，项目和聊天的空闲流只能按保活周期发送无正文心跳。
///
/// 数据构造：同一只读手机订阅项目和run-a聊天；另一隔离设备触发30次变更，间隔100ms。
///   30×100ms=3s，跨过一次2s心跳；基线排除，允许floor(实测时长/2s)+1个边界心跳。
/// 执行过程：两条WSS基线 → 持续读取 → 其他设备重复撤销唤醒 → 撤销订阅手机 → 统计帧和JSON字节。
/// 预期结果：两条流都有保活但不随每次无关变更发送；序号仍0，无正文、无权限变化，撤销回收。
#[test]
fn unrelated_changes_do_not_flood_project_or_chat_watches() {
    check(None, true);
}

/// 原生客户端在一次受控断流后重建聊天基线，随后撤销权限必须终止读取。
///
/// 数据构造：相同TLS/SQLite只读fixture；一个文件标记触发一次unavailable，后续正文改为“恢复后”。
/// 执行过程：配对 → 真实Swift订阅 → 中断 → recovering清空 → 新基线 → 撤销。
/// 预期结果：不能沿用旧正文或重放写操作；新正文准确；撤销后客户端明确unauthorized。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn native_conversation_watch_recovers_and_revokes() {
    check(
        Some(std::env::var("NORMA_SWIFT_PROBE").expect("显式原生probe")),
        false,
    );
}

fn check(probe: Option<String>, noisy: bool) {
    let dir = tempfile::tempdir().expect("隔离项目");
    let store = AppStore::new(dir.path().join("app.db"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Chat")
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
        .expect("只读授权");
    if noisy {
        store
            .mobile_access_store()
            .register(
                "wake",
                "Other phone",
                &"b".repeat(64),
                &[MobileProjectAccess {
                    workspace_id: workspace.clone(),
                    access: MobilePermission::Read,
                }],
            )
            .expect("仅用于触发无关变更的测试设备");
    }
    let identity = ServerIdentity::generate().expect("TLS");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let page=Arc::new(Mutex::new(ConversationPage::from_codex(json!({"data":[{"id":"t","items":[{"id":"a","type":"agentMessage","text":"中"}]}],"nextCursor":null})).expect("页")));
    let (history, histories) = async_channel::bounded::<LiveConversationRequest>(8); // eight matches existing peer cap.
    let (snapshots, reads) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let pages = page.clone();
    let interruption = dir.path().join("chat-interrupt");
    let history_worker = std::thread::spawn(move || {
        let mut interrupted = false;
        while let Ok(call) = histories.recv_blocking() {
            if !interrupted && interruption.exists() {
                interrupted = true;
                pages.lock().expect("更换恢复基线").items[0].text = "恢复后".into();
                let _ = call.reply.send(Err("unavailable"));
                continue;
            }
            let _ = call.reply.send(Ok(pages.lock().expect("页").clone()));
        }
    });
    let snapshot_worker = std::thread::spawn(move || {
        while let Ok(mut call) = reads.recv_blocking() {
            let p = &mut call.projects[0];
            p.state = "open".into();
            p.route_id = Some("route-a".into());
            p.card_count = Some(1);
            p.cards = call
                .include_cards
                .then_some(vec![norma::mobile::CardSnapshot {
                    node_id: 7,
                    profile: "Codex".into(),
                    title: "Chat".into(),
                    mobile_control: true,
                    can_interrupt: None,
                    task_timing: None,
                    task_progress: None,
                    task_summary: None,
                    state: "ready".into(),
                    run_id: Some("run-a".into()),
                    session_id: Some("session-a".into()),
                    turn_id: None,
                    task_title: None,
                }]);
            let _ = call.reply.send(call.projects);
        }
    });
    let server = MobileServer::start_with_conversations(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(snapshots),
        None,
        Some(history),
    )
    .expect("服务");
    if noisy {
        check_idle_noise(&server, &machine, &workspace, cert);
        server.shutdown();
        drop(server);
        history_worker.join().expect("回收history");
        snapshot_worker.join().expect("回收snapshot");
        return;
    }
    if let Some(probe) = probe {
        run_native(&server, dir.path(), &workspace, &probe);
        assert!(
            dir.path().join("chat-interrupt").exists(),
            "原生端必须请求受控断流"
        );
        assert_eq!(
            page.lock().expect("恢复正文").items[0].text,
            "恢复后",
            "新基线不能沿用旧正文"
        );
        server.shutdown();
        drop(server);
        history_worker.join().expect("回收history");
        snapshot_worker.join().expect("回收snapshot");
        return;
    }
    let action = json!({"type":"watch_conversation","device_id":"phone-a","token":"a".repeat(64),"request":{"workspace_id":workspace,"route_id":"route-a","node_id":7,"run_id":"run-a","session_id":"session-a","cursor":null}});
    let mut socket = support::connect(server.addresses()[0], cert);
    let baseline = support::exchange(&mut socket, &machine, "watch", action);
    assert_eq!(
        baseline["result"]["status"], "conversation_event",
        "必须是持续订阅协议"
    );
    let first = &baseline["result"]["conversation_event"];
    assert_eq!(first["sequence"], 0, "连接从零开始");
    assert_eq!(first["page"]["items"][0]["text"], "中", "初始完整正文");
    page.lock().expect("更新页").items[0].text = "中文".into();
    let mut receive = || -> Value {
        serde_json::from_str(
            socket
                .read()
                .expect("两秒内收到新核验")
                .to_text()
                .expect("JSON"),
        )
        .expect("解析")
    };
    let update = receive();
    let next = &update["result"]["conversation_event"];
    assert_eq!(next["stream_id"], first["stream_id"], "同连接不能改变来源");
    assert_eq!(next["sequence"], 1, "正文变化推进一次");
    assert_eq!(next["page"]["items"][0]["text"], "中文", "不残留旧短消息");
    let beat = receive();
    assert_eq!(
        beat["result"]["conversation_event"]["sequence"], 1,
        "心跳不伪造新变化"
    );
    assert!(
        beat["result"]["conversation_event"]["page"].is_null(),
        "心跳不能重复整页"
    );
    server.revoke("phone-a").expect("撤销");
    assert!(socket.read().is_err(), "撤销必须终止已认证订阅");
    drop(socket);
    server.shutdown();
    drop(server);
    history_worker.join().expect("回收history");
    snapshot_worker.join().expect("回收snapshot");
}

fn check_idle_noise(server: &MobileServer, machine: &str, workspace: &str, cert: Vec<u8>) {
    use std::time::{Duration, Instant};
    let mut project = support::connect(server.addresses()[0], cert.clone());
    let baseline = support::exchange(
        &mut project,
        machine,
        "idle-project",
        json!({
        "type":"watch_project","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace}),
    );
    assert_eq!(
        baseline["result"]["event"]["kind"], "snapshot",
        "项目必须先建立完整基线"
    );
    let mut chat = support::connect(server.addresses()[0], cert);
    let baseline = support::exchange(
        &mut chat,
        machine,
        "idle-chat",
        json!({
        "type":"watch_conversation","device_id":"phone-a","token":"a".repeat(64),
        "request":{"workspace_id":workspace,"route_id":"route-a","node_id":7,"run_id":"run-a","session_id":"session-a","cursor":null}}),
    );
    assert_eq!(
        baseline["result"]["conversation_event"]["page"]["items"][0]["text"], "中",
        "聊天必须包含原始准确基线"
    );
    let project_reader = std::thread::spawn(move || {
        let mut frames = 0;
        let mut bytes = 0;
        while let Ok(frame) = project.read() {
            let text = frame.to_text().expect("项目文本帧");
            let value: Value = serde_json::from_str(text).expect("项目JSON");
            assert_eq!(
                value["result"]["event"]["kind"], "heartbeat",
                "无关变更不能伪造项目delta"
            );
            assert_eq!(
                value["result"]["event"]["sequence"], 0,
                "无关变更不能推进项目序号"
            );
            assert_eq!(
                value["result"]["grant"]["revision"], 1,
                "另一设备变更不能改写本手机权限"
            );
            assert!(
                value["result"]["event"].get("project").is_none(),
                "心跳不能重传卡片"
            );
            frames += 1;
            bytes += text.len();
        }
        (frames, bytes)
    });
    let chat_reader = std::thread::spawn(move || {
        let mut frames = 0;
        let mut bytes = 0;
        while let Ok(frame) = chat.read() {
            let text = frame.to_text().expect("聊天文本帧");
            let value: Value = serde_json::from_str(text).expect("聊天JSON");
            assert_eq!(
                value["result"]["status"], "conversation_event",
                "空闲聊天流不能失效"
            );
            assert_eq!(
                value["result"]["conversation_event"]["sequence"], 0,
                "无关变更不能推进聊天序号"
            );
            assert!(
                value["result"]["conversation_event"]["page"].is_null(),
                "心跳不能重传正文"
            );
            frames += 1;
            bytes += text.len();
        }
        (frames, bytes)
    });
    let started = Instant::now();
    // 30×100ms=3s：10Hz无关唤醒；被测设备始终有效，最后才显式撤销。
    for _ in 0..30 {
        server.revoke("wake").expect("无关设备撤销的正式变更信号");
        std::thread::sleep(Duration::from_millis(100));
    }
    let elapsed = started.elapsed();
    server.revoke("phone-a").expect("结束当前设备的两条流");
    let project = project_reader.join().expect("项目读取结束");
    let chat = chat_reader.join().expect("聊天读取结束");
    let maximum = elapsed.as_secs() / 2 + 1; // 每2s一次，另给开始/结束边界1次余量。
    eprintln!(
        "IDLE_WATCH_TRAFFIC={}",
        json!({"wall_seconds":elapsed.as_secs_f64(),"unrelated_changes":30,
        "project_frames":project.0,"project_json_bytes":project.1,"chat_frames":chat.0,"chat_json_bytes":chat.1,
        "scope":"Two Rust loopback WSS readers; counts UTF-8 application messages only, excluding baseline/TLS/TCP headers; no Provider"})
    );
    assert!(project.0 >= 1, "项目流仍需定期保活");
    assert!(chat.0 >= 1, "聊天流仍需定期保活");
    assert!(
        project.0 <= maximum,
        "项目心跳不能跟随每次无关变化发送：{}帧，最多{}",
        project.0,
        maximum
    );
    assert!(
        chat.0 <= maximum,
        "聊天心跳不能跟随每次无关变化发送：{}帧，最多{}",
        chat.0,
        maximum
    );
}

fn run_native(server: &MobileServer, directory: &std::path::Path, workspace: &str, probe: &str) {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let qr = directory.join("offer.json");
    std::fs::write(
        &qr,
        server
            .begin_pairing("Chat Mac")
            .expect("配对")
            .to_json()
            .expect("JSON"),
    )
    .expect("临时QR");
    std::fs::set_permissions(&qr, std::fs::Permissions::from_mode(0o600)).expect("限制读取");
    let log = directory.join("native.log");
    let mut child = std::process::Command::new(probe)
        .arg(&qr)
        .arg("chat-events")
        .stdout(std::fs::File::create(&log).expect("日志"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("原生检查");
    // 120s =90s Xcode启动 +30s配对预算；无常驻测试进程。
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut device = None;
    let mut revoked = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("进程状态") {
            break status;
        }
        if let Some(pending) = server.pending().expect("申请") {
            assert!(device.is_none(), "测试仅配对一次");
            let grant = server
                .approve(
                    &pending.request_id,
                    &[MobileProjectAccess {
                        workspace_id: workspace.into(),
                        access: MobilePermission::Read,
                    }],
                )
                .expect("只读授权");
            device = Some(grant.device_id);
        }
        if !revoked && directory.join("chat-revoke").exists() {
            server
                .revoke(device.as_deref().expect("已配对手机"))
                .expect("撤销");
            revoked = true;
        }
        if Instant::now() >= deadline {
            child.kill().expect("停止超时");
            child.wait().expect("回收");
            panic!("原生聊天恢复检查超时");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let evidence =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/norma-ios-evidence");
    std::fs::create_dir_all(&evidence).expect("工作区证据目录");
    let platform = if std::env::var_os("NORMA_IOS_SIMULATOR").is_some() {
        "ios"
    } else {
        "macos"
    };
    std::fs::copy(
        &log,
        evidence.join(format!("chat-recovery-native-{platform}-v61.log")),
    )
    .expect("保留原生恢复证据");
    let output = std::fs::read_to_string(log).expect("日志");
    assert!(status.success(), "原生聊天恢复失败：{output}");
    assert!(
        output.contains("NATIVE_CHAT_RECOVERY_REVOKE_OK"),
        "必须有完整恢复与撤销证据：{output}"
    );
    assert!(revoked, "不能跳过真实服务撤销");
}
