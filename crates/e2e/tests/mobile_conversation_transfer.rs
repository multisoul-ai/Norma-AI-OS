use app::{LiveConversationRequest, LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{ConversationPage, ServerIdentity};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 大于旧500KiB上限的正文与工具输出分片传输，完整组装后仍绑定原请求。
///
/// 数据构造：中文3字节×200000=600000字节，另600000字符的工具输出；隔离只读手机与准确卡片fixture。
///   无关BEGIN IMMEDIATE事务持有写锁，SQLite仍允许读取此前已提交的授权和项目。
/// 执行过程：持有写锁 → 真实TLS/WSS请求 → 前后台权限/运行核验 → 连续分片 → 原JSON重组 → 释放锁并撤销后再请求。
/// 预期结果：鉴权及项目读取不等待写锁；每帧<500KiB；正文/工具字节准确且无截断；撤销后无内容，不访问真实Provider或用户文件。
#[test]
fn conversation_transfer_crosses_frame_limit_without_truncation() {
    check(None, false);
}

/// 原生Foundation在真实Rust WSS上无损读取大于500KiB的响应；不替代真实Provider/GPUI验收。
///
/// 数据构造：与网络检查相同的精确Unicode/工具fixture、独立测试Keychain凭据。
/// 执行过程：仅测试配对read → Swift分片重组 → 校验正文/工具 → 旧run拒绝。
/// 预期结果：完整响应准确，无半条消息或错会话内容；测试进程有界回收。
/// 显式MEASURE=1另取20次原生首片/全文与6s空闲窗口；CPU含命名服务与共享运行时线程，系统字节不冒充JSON或线速。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn native_large_conversation_transport() {
    check(
        Some(std::env::var("NORMA_SWIFT_PROBE").expect("显式原生probe")),
        false,
    );
}

/// 本机大聊天响应分别记录连接、首个完整WSS分片与全文校验耗时，不把整页时间当首包。
///
/// 数据构造：复用600000字节中文与引号/换行工具fixture；持有无关写锁，20次独立连接，无新增模型任务。
///   20次允许nearest-rank p50取第10项、p95取第19项；原样保留全部样本。
/// 执行过程：每次TLS/WSS连接 → 发送 → 记录首帧到达（JSON解析前） → 无损重组/精确正文 → 原路由竞争/撤销检查。
/// 预期结果：每次全文与工具内容都精确，不能只收到首帧便算成功；失效运行不能收到完整尾片。
/// 范围：Rust回环客户端与前台fixture，非原生SwiftUI、模型首token或物理网络；JSON字节不含TLS/TCP开销。
#[test]
#[ignore = "dedicated first-fragment timing; run without concurrent builds/tests"]
fn conversation_transfer_first_fragment_measurement() {
    check(None, true);
}

fn check(probe: Option<String>, measuring: bool) {
    let measuring = measuring
        || (probe.is_some() && std::env::var("NORMA_MOBILE_MEASURE").as_deref() == Ok("1"));
    let dir = tempfile::tempdir().expect("隔离库");
    let store = AppStore::new(dir.path().join("app.db"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Large")
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
    let identity = ServerIdentity::generate().expect("TLS");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let (history, histories) = async_channel::bounded::<LiveConversationRequest>(8); // matches eight peer slots.
    let (snapshots, reads) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let body = "长".repeat(200_000);
    let details = "\\\"\n".repeat(200_000);
    let page = ConversationPage::from_codex(json!({"data":[{"id":"turn-a","items":[
        {"id":"a","type":"agentMessage","text":body},
        {"id":"tool","type":"commandExecution","aggregatedOutput":details}
    ]}],"nextCursor":null}))
    .expect("冻结完整内容");
    let history_worker = std::thread::spawn(move || {
        while let Ok(request) = histories.recv_blocking() {
            let _ = request.reply.send(Ok(page.clone()));
        }
    });
    let invalidate = Arc::new(AtomicBool::new(false));
    let reads_after_arm = Arc::new(AtomicUsize::new(0));
    let invalidating = invalidate.clone();
    let read_count = reads_after_arm.clone();
    let snapshot_worker = std::thread::spawn(move || {
        while let Ok(mut request) = reads.recv_blocking() {
            let changed = invalidating.load(Ordering::SeqCst)
                && read_count.fetch_add(1, Ordering::SeqCst) > 0;
            let project = &mut request.projects[0];
            project.state = "open".into();
            project.route_id = Some("route-a".into());
            project.card_count = Some(1);
            project.cards = request
                .include_cards
                .then_some(vec![norma::mobile::CardSnapshot {
                    node_id: 7,
                    profile: "Codex".into(),
                    title: "Test".into(),
                    mobile_control: true,
                    can_interrupt: None,
                    task_timing: None,
                    task_progress: None,
                    task_summary: None,
                    state: "ready".into(),
                    run_id: Some(if changed { "replacement-run" } else { "run-a" }.into()),
                    session_id: Some("session-a".into()),
                    turn_id: None,
                    task_title: None,
                }]);
            let _ = request.reply.send(request.projects);
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
    if let Some(probe) = probe {
        if measuring {
            std::fs::write(dir.path().join("measure-latency"), "").expect("显式隔离测量标记");
        }
        let qr = dir.path().join("offer.json");
        std::fs::write(
            &qr,
            server
                .begin_pairing("Large Mac")
                .expect("QR")
                .to_json()
                .expect("编码"),
        )
        .expect("写入临时QR");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&qr, std::fs::Permissions::from_mode(0o600))
            .expect("仅本用户可读");
        let log = dir.path().join("native.log");
        let evidence = std::env::var_os("NORMA_MOBILE_EVIDENCE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/norma-ios-evidence")
                    .join(format!(
                        "native-transfer-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("系统时钟")
                            .as_nanos()
                    ))
            });
        assert!(evidence.is_absolute(), "测量证据必须使用绝对路径");
        // 每次独立创建，不覆盖v61或其他已有测量；失败也保留原始证据。
        std::fs::create_dir_all(evidence.parent().expect("证据目录必须有父目录"))
            .expect("准备证据父目录");
        std::fs::create_dir(&evidence).expect("创建全新测量证据目录");
        let platform = if std::env::var_os("NORMA_IOS_SIMULATOR").is_some() {
            "ios"
        } else {
            "macos"
        };
        let mut child = std::process::Command::new(probe)
            .arg(&qr)
            .arg("large")
            .stdout(std::fs::File::create(&log).expect("日志"))
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("原生probe");
        // 120s =90s Xcode startup allowance +30s pairing window; no unlimited child process.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut approved = false;
        let mut sampled = false;
        let status = loop {
            if let Some(status) = child.try_wait().expect("进程状态") {
                break status;
            }
            if let Some(pending) = server.pending().expect("申请") {
                assert!(!approved, "一个probe只能配对一次");
                server
                    .approve(
                        &pending.request_id,
                        &[MobileProjectAccess {
                            workspace_id: workspace.clone(),
                            access: MobilePermission::Read,
                        }],
                    )
                    .expect("隔离只读授权");
                approved = true;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().expect("停止超时");
                child.wait().expect("回收");
                panic!("原生长响应检查超时");
            }
            if measuring && !sampled && dir.path().join("native-idle-ready").exists() {
                let result = std::process::Command::new("python3")
                    .arg(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../scripts/measure-norma-mobile-idle.py"
                    ))
                    .arg(std::process::id().to_string())
                    .arg(evidence.join(platform))
                    .output();
                if !result.as_ref().is_ok_and(|output| output.status.success()) {
                    child.kill().expect("测量失败停止原生子进程");
                    child.wait().expect("回收原生子进程");
                    panic!("服务线程/网络测量失败：{result:?}");
                }
                std::fs::write(dir.path().join("native-idle-done"), "").expect("有界测量完成");
                sampled = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        std::fs::copy(
            &log,
            evidence.join(format!("first-fragment-native-{platform}.log")),
        )
        .expect("保存原生分片检查输出");
        let output = std::fs::read_to_string(log).expect("读取日志");
        assert!(status.success(), "原生长响应检查失败：{output}");
        assert!(
            output.contains("NATIVE_LARGE_CONVERSATION_OK"),
            "需要完整正文和旧run拒绝的证据"
        );
        if measuring {
            assert!(
                sampled,
                "服务owner必须完成独立CPU/系统流量测量，不能只采客户端"
            );
            assert!(
                output.contains("NATIVE_IDLE_CHECKPOINT_OK"),
                "显式测量必须持续消费空闲订阅并取得服务线程CPU/系统字节证据：{output}"
            );
            assert!(
                output.contains("NATIVE_FIRST_FRAGMENT_TIMING_OK"),
                "显式测量必须取得原生首分片、全文及字节记录，不能拿整页读取代替：{output}"
            );
        }
        server.shutdown();
        drop(server);
        history_worker.join().expect("回收历史线程");
        snapshot_worker.join().expect("回收状态线程");
        return;
    }
    let action = json!({"type":"conversation","device_id":"phone-a","token":"a".repeat(64),"request":{
        "workspace_id":workspace,"route_id":"route-a","node_id":7,"run_id":"run-a","session_id":"session-a","cursor":null}});
    let writer = rusqlite::Connection::open(dir.path().join("app.db")).expect("隔离并发写连接");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("持有写锁，已提交内容仍应可读");
    let mut samples = Vec::new();
    // Ordinary regression keeps one full read; the dedicated benchmark takes20 independent samples.
    for _ in 0..if measuring { 20 } else { 1 } {
        let connected_at = std::time::Instant::now();
        let mut socket = support::connect(server.addresses()[0], cert.clone());
        let connect_ms = connected_at.elapsed().as_secs_f64() * 1000.0;
        let request = json!({"version":1,"request_id":"read-large","machine_id":machine,"action":action.clone()});
        let started = std::time::Instant::now();
        socket
            .send(tungstenite::Message::text(request.to_string()))
            .expect("发送大页请求");
        let first = socket.read().expect("首个完整WSS分片");
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        let text = first.to_text().expect("首帧文本");
        assert!(text.len() < 500 * 1024, "首帧也必须遵守转义后预算");
        let mut frames = 1;
        let mut json_bytes = text.len();
        let mut response: Value = serde_json::from_str(text).expect("首帧JSON");
        let mut complete = String::new();
        let mut transfer_id = None;
        loop {
            assert_eq!(
                response["result"]["status"], "conversation_fragment",
                "大页必须分片，不得以history_too_large拒绝"
            );
            let part = &response["result"]["fragment"];
            if let Some(id) = &transfer_id {
                assert_eq!(&part["transfer_id"], id, "不能混入其他冻结读取");
            } else {
                transfer_id = Some(part["transfer_id"].clone());
            }
            assert_eq!(
                part["offset"].as_u64(),
                Some(complete.len() as u64),
                "每片无缺口不重复"
            );
            complete.push_str(part["text"].as_str().expect("完整UTF-8片段"));
            if Some(complete.len() as u64) == part["total_bytes"].as_u64() {
                break;
            }
            let frame = socket.read().expect("下一片");
            let text = frame.to_text().expect("JSON文本");
            assert!(text.len() < 500 * 1024, "转义后仍低于frame预算");
            frames += 1;
            json_bytes += text.len();
            response = serde_json::from_str(text).expect("解析");
        }
        let decoded: Value = serde_json::from_str(&complete).expect("完整响应");
        assert_eq!(
            decoded["request"]["run_id"], "run-a",
            "完整内容仍属于原运行"
        );
        assert_eq!(
            decoded["page"]["items"][0]["text"], body,
            "长正文不应被截断或替换摘要"
        );
        let tool: Value = serde_json::from_str(
            decoded["page"]["items"][1]["details"]
                .as_str()
                .expect("工具详情"),
        )
        .expect("原工具JSON");
        assert_eq!(
            tool["aggregatedOutput"], details,
            "工具中的引号/换行不能在重组时损坏"
        );
        samples.push(json!({"connect_ms":connect_ms,"first_fragment_ms":first_ms,
            "page_validated_ms":started.elapsed().as_secs_f64()*1000.0,"frames":frames,"json_bytes":json_bytes,"payload_bytes":complete.len()}));
        drop(socket);
    }
    writer
        .execute_batch("ROLLBACK")
        .expect("释放写锁以验证后续撤销");
    if measuring {
        eprintln!(
            "CONVERSATION_FRAGMENT_TIMING={}",
            json!({"samples":samples,
            "scope":"Rust loopback WSS with unrelated SQLite writer held; first complete application frame before JSON decode; frozen Unicode/tool fixture, no Provider; byte counts exclude baseline/TLS/TCP headers"})
        );
    }
    // First validation sees run-a; final validation sees a replacement run, after some fragments.
    invalidate.store(true, Ordering::SeqCst);
    let mut changing = support::connect(server.addresses()[0], cert.clone());
    let mut partial = support::exchange(&mut changing, &machine, "changing", action.clone());
    let mut received = 0;
    let mut expected = 0;
    while partial["result"]["status"] == "conversation_fragment" {
        let fragment = &partial["result"]["fragment"];
        received += fragment["text"].as_str().expect("部分内容").len();
        expected = fragment["total_bytes"].as_u64().expect("声明总量") as usize;
        let frame = changing.read().expect("最后一片必须被拒绝");
        partial = serde_json::from_str(frame.to_text().expect("JSON")).expect("解析");
    }
    assert_eq!(
        partial["error"], "session_changed",
        "运行替换必须中止冻结读取的提交"
    );
    assert!(
        received > 0 && received < expected,
        "客户端只能收到不完整缓冲，不能形成可显示响应"
    );
    assert!(partial.get("result").is_none(), "拒绝中不能混入旧内容尾片");
    drop(changing);
    server.revoke("phone-a").expect("撤销");
    let mut denied = support::connect(server.addresses()[0], cert);
    let result = support::exchange(&mut denied, &machine, "denied", action);
    assert_eq!(result["error"], "unauthorized", "撤销后不能开始长读取");
    assert!(result.get("result").is_none(), "撤销响应不能附带内容");
    drop(denied);
    server.shutdown();
    drop(server);
    history_worker.join().expect("回收history线程");
    snapshot_worker.join().expect("回收snapshot线程");
}
