use app::{
    LiveApprovalRequest, LiveSnapshotRequest, MobileAccess, MobileServer, MobileSources,
    PushRelayConfig,
};
use norma::mobile::{
    ApprovalSnapshot, ApprovalTarget, CardSnapshot, ProjectSnapshot, ServerIdentity,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{
    AppStore, ApprovalAuditStore, BackgroundApprovalAudit, MobilePermission, MobileProjectAccess,
    MobilePushRegistration, MobilePushStore, MobileTurnKey, MobileTurnOutcome, MobileTurnState,
    MobileTurnStore, PushEnvironment,
};

/// 真实Mac通知worker只投递可访问项目的审批与已提交终态；未知结果不重发，撤销/关窗停止新发送。
///
/// 数据构造：A项目只读手机；另一项目/环境手机；phone-revoked在取到订阅快照后撤销；同run下两个独立终态。
/// 执行过程：真实WSS host启通知 → 审批被旧relay以400拒绝 → 停用并更换地址/凭据 → 后台审计提交完成/失败 → relay断开/410 → 撤销/关窗。
/// 预期结果：仅3个有界事件，含项目/卡片/各自完成摘要，协议拒绝及切换不重发审批/停用期间的终态；新端点不收到旧凭据，配对授权不变；410仅清订阅，停止后无请求。
#[test]
fn push_worker_uses_durable_turns_current_grants_and_never_replays_unknown_sends() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    std::fs::create_dir(dir.path().join("other")).expect("B项目目录");
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Private project")
        .expect("A项目");
    let other = store
        .workspace_identity_store()
        .resolve_or_create(&dir.path().join("other"), "Other")
        .expect("B项目");
    let registration = MobilePushRegistration {
        token: "d".repeat(64),
        environment: PushEnvironment::Sandbox,
    };
    let grants = store.mobile_access_store();
    let phone = grants
        .register(
            "phone-read",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读手机");
    let foreign = grants
        .register(
            "phone-other",
            "Other",
            &"b".repeat(64),
            &[MobileProjectAccess {
                workspace_id: other.workspace_id,
                access: MobilePermission::Read,
            }],
        )
        .expect("另一项目手机");
    let production = grants
        .register(
            "phone-production",
            "Production",
            &"c".repeat(64),
            &phone.projects,
        )
        .expect("另一环境");
    let push = MobilePushStore::new(store.path().to_path_buf());
    let revoked = grants
        .register("phone-revoked", "Revoked", &"9".repeat(64), &phone.projects)
        .expect("待撤销手机");
    push.replace(
        &revoked,
        0,
        Some(&MobilePushRegistration {
            token: "9".repeat(64),
            ..registration.clone()
        }),
    )
    .expect("待撤销订阅")
    .expect("成功");
    push.replace(&phone, 0, Some(&registration))
        .expect("注册")
        .expect("成功");
    push.replace(
        &foreign,
        0,
        Some(&MobilePushRegistration {
            token: "e".repeat(64),
            ..registration.clone()
        }),
    )
    .expect("另一手机")
    .expect("成功");
    push.replace(
        &production,
        0,
        Some(&MobilePushRegistration {
            token: "f".repeat(64),
            environment: PushEnvironment::Production,
        }),
    )
    .expect("另一环境")
    .expect("成功");
    let target = ApprovalTarget {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-approval".into(),
        request_id: "request-a".into(),
    };
    let card = CardSnapshot {
        node_id: 7,
        profile: "Codex".into(),
        title: "PRIVATE CARD".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: Some("另一回合的内容，不应进入这次通知".into()),
        state: "done".into(),
        run_id: Some("run-a".into()),
        session_id: Some("session-a".into()),
        turn_id: Some("latest-turn".into()),
        task_title: Some("PRIVATE TASK".into()),
    };
    let (snapshots, receive) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let (ready, observed) = std::sync::mpsc::channel();
    let workspace = project.workspace_id.clone();
    let source = std::thread::spawn(move || {
        while let Ok(request) = receive.recv_blocking() {
            let projects = request
                .projects
                .into_iter()
                .map(|p| {
                    if p.workspace_id == workspace {
                        ProjectSnapshot {
                            state: "open".into(),
                            route_id: Some("route-a".into()),
                            cards: Some(vec![card.clone()]),
                            card_count: Some(1),
                            ..p
                        }
                    } else {
                        p
                    }
                })
                .collect();
            let _ = request.reply.send(projects);
            let _ = ready.send(());
        }
    });
    let pending = Arc::new(AtomicBool::new(true));
    let waiting = pending.clone();
    let (approvals, receive) = async_channel::bounded::<LiveApprovalRequest>(8);
    let access_store = store.mobile_access_store();
    let approval_source = std::thread::spawn(move || {
        let mut revoke_once = true;
        while let Ok(request) = receive.recv_blocking() {
            if revoke_once {
                access_store
                    .revoke("phone-revoked")
                    .expect("读取订阅后撤销");
                revoke_once = false;
            }
            let items = if waiting.load(Ordering::Acquire) {
                vec![ApprovalSnapshot {
                    target: target.clone(),
                    card_title: "PRIVATE TITLE".into(),
                    tool: "PRIVATE TOOL".into(),
                    details: json!({"command":"PRIVATE COMMAND"}),
                    decisions: vec![norma::agent_hook::ApprovalDecision::Allow],
                }]
            } else {
                vec![]
            };
            let _ = request.reply.send(Ok(items));
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").expect("本地relay");
    let endpoint = format!(
        "http://{}/norma/v1/notifications",
        listener.local_addr().expect("地址")
    );
    let replacement = TcpListener::bind("127.0.0.1:0").expect("第二个relay");
    let replacement_endpoint = format!(
        "http://{}/norma/v1/notifications",
        replacement.local_addr().expect("第二地址")
    );
    let (received, events) = std::sync::mpsc::channel();
    let relay = std::thread::spawn(move || {
        // 精确接收3个事件后关闭listener，多发会显式连接失败，不触达任何外部服务。
        for index in 0..3 {
            let current = if index == 0 { &listener } else { &replacement };
            let (socket, _) = current.accept().expect("接收relay请求");
            // 5s=本地测试IO最大预算，防止错误请求永久占住测试线程。
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("读预算");
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).expect("请求行");
            assert_eq!(
                line, "POST /norma/v1/notifications HTTP/1.1\r\n",
                "必须发送到唯一通知入口"
            );
            let mut length = 0;
            let mut authorized = false;
            loop {
                line.clear();
                reader.read_line(&mut line).expect("请求头");
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse::<usize>().expect("长度");
                    }
                    if key.eq_ignore_ascii_case("authorization") {
                        let key = if index == 0 { "1" } else { "2" };
                        authorized = value.trim() == format!("Bearer {}", key.repeat(64));
                    }
                }
            }
            assert!(authorized, "Mac必须使用单独relay凭据");
            assert!(length > 0 && length <= 4096, "请求体必须有界");
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).expect("请求体");
            let event: Value = serde_json::from_slice(&bytes).expect("JSON");
            let kind = event["kind"].as_str().expect("事件类型").to_owned();
            received.send(event).expect("记录事件");
            // completed模拟请求已接收但响应丢失，必须不自动重发。
            if kind == "completed" {
                continue;
            }
            let (status, body) = if kind == "failed" {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("时钟")
                    .as_millis();
                (
                    "410 Gone",
                    json!({"error":"token_unregistered","invalidated_at":now}).to_string(),
                )
            } else {
                (
                    "400 Bad Request",
                    json!({"error":"invalid_event"}).to_string(),
                )
            };
            write!(reader.get_mut(), "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("relay回执");
        }
    });
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(
            store.clone(),
            ServerIdentity::generate().expect("身份"),
        ),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            snapshots: Some(snapshots),
            approvals: Some(approvals),
            ..Default::default()
        },
    )
    .expect("WSS host");
    server
        .start_push_notifications(
            PushRelayConfig::for_e2e(&endpoint, &"1".repeat(64), PushEnvironment::Sandbox)
                .expect("隔离relay配置"),
        )
        .expect("启动通知");
    // 10s=1s轮询+2s前台读取+本地HTTP与构建机调度余量；不依赖固定sleep先后。
    let budget = Duration::from_secs(10);
    observed.recv_timeout(budget).expect("worker已建立终态游标");
    let approval = events.recv_timeout(budget).expect("审批事件");
    assert_eq!(approval["kind"], "approval", "真实可决策请求产生提醒");
    assert_eq!(approval["request_id"], "request-a", "必须定位准确审批");
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("project.sqlite")).with_turn_store(
            MobileTurnStore::for_workspace(
                store.path().to_path_buf(),
                project.workspace_id.clone(),
            ),
        ),
    )
    .expect("真实审计writer");
    let outcome = |turn: &str, state| MobileTurnOutcome {
        summary: Some(
            if state == MobileTurnState::Completed {
                "已修复通知跳转，回归测试通过。"
            } else {
                "测试失败：连接已断开。"
            }
            .into(),
        ),
        key: MobileTurnKey {
            node_id: 7,
            profile: "Codex".into(),
            run_id: "run-a".into(),
            session_id: "session-a".into(),
            turn_id: turn.into(),
        },
        state,
    };
    server
        .configure_push_notifications(None)
        .expect("单独停用通知");
    assert_eq!(
        server.devices().expect("设备").len(),
        4,
        "停用通知不得移除配对记录"
    );
    writer
        .record_turn(outcome("turn-while-disabled", MobileTurnState::Completed))
        .expect("停用期间完成入队")
        .recv_timeout(budget)
        .expect("停用期间回执")
        .expect("停用期间提交");
    while observed.try_recv().is_ok() {}
    server
        .configure_push_notifications(Some(
            PushRelayConfig::for_e2e(
                &replacement_endpoint,
                &"2".repeat(64),
                PushEnvironment::Sandbox,
            )
            .expect("新凭据"),
        ))
        .expect("替换服务");
    observed.recv_timeout(budget).expect("切换后重新建立水位");
    // Keep the same approval through one more tick: 1Hz polling must not replay an attempted request.
    observed.recv_timeout(budget).expect("切换后再次读取");
    assert!(
        events.try_recv().is_err(),
        "不能重放仍等待的审批或停用期间已完成的任务"
    );
    pending.store(false, Ordering::Release);
    writer
        .record_turn(outcome("turn-completed", MobileTurnState::Completed))
        .expect("完成入队")
        .recv_timeout(budget)
        .expect("完成回执")
        .expect("完成提交");
    writer
        .record_turn(outcome("turn-failed", MobileTurnState::Failed))
        .expect("失败入队")
        .recv_timeout(budget)
        .expect("失败回执")
        .expect("失败提交");
    let completed = events.recv_timeout(budget).expect("完成通知");
    let failed = events.recv_timeout(budget).expect("失败通知");
    assert_eq!(completed["kind"], "completed", "不能把丢响应的完成重新发送");
    assert_eq!(
        completed["turn_id"], "turn-completed",
        "下一回合开始后旧完成仍定位原回合"
    );
    assert_eq!(failed["kind"], "failed", "失败必须来自单独已提交事件");
    assert_eq!(failed["turn_id"], "turn-failed", "不能用卡片done伪造完成");
    assert_eq!(
        failed["token"], registration.token,
        "其他项目或APNs环境手机不得收到A事件"
    );
    assert_eq!(
        approval["token"], registration.token,
        "取快照后撤销的手机不能收到审批"
    );
    assert_eq!(
        completed["token"], registration.token,
        "撤销后不能收到下一项完成事件"
    );
    assert!(failed.get("request_id").is_none(), "终态事件不含审批ID");
    assert_eq!(
        completed["preview"]["project"], "Private project",
        "通知必须显示所属项目"
    );
    assert_eq!(
        completed["preview"]["card"], "PRIVATE CARD · #7",
        "同名卡片也须由节点编号区分"
    );
    assert_eq!(
        completed["preview"]["summary"], "已修复通知跳转，回归测试通过。",
        "必须使用已完成回合冻结的最终输出"
    );
    assert_eq!(
        failed["preview"]["summary"], "测试失败：连接已断开。",
        "失败不能借用完成回合的摘要"
    );
    assert!(
        !format!("{approval}{completed}{failed}").contains("PRIVATE COMMAND"),
        "不能附带工具参数"
    );
    assert!(
        !completed.to_string().contains("另一回合的内容"),
        "通知不能误用卡片下一回合的摘要"
    );
    relay.join().expect("relay结束");
    let deadline = std::time::Instant::now() + budget;
    while push
        .get(&phone)
        .expect("订阅状态")
        .expect("授权保留")
        .registration
        .is_some()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        push.get(&phone)
            .expect("410后")
            .expect("仍授权")
            .registration
            .is_none(),
        "APNs当前失效必须清除token"
    );
    assert!(
        !grants
            .list()
            .expect("授权列表")
            .iter()
            .find(|g| g.device_id == "phone-read")
            .expect("phone")
            .revoked,
        "APNs失效不能撤销项目权限"
    );
    server.revoke("phone-read").expect("Mac撤销");
    server.shutdown();
    assert!(
        server.configure_push_notifications(None).is_err(),
        "关窗后的迟到配置不能重新启用通知worker"
    );
    drop(server);
    source.join().expect("快照源释放");
    approval_source.join().expect("审批源释放");
    assert!(
        events.try_recv().is_err(),
        "失效、撤销、关闭不能产生重复投递"
    );
}
