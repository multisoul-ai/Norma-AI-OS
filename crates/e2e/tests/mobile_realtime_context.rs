use app::{LiveCommandRequest, LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::CardSnapshot;
use serde_json::{json, Value};
use std::time::Duration;
use storage::{AppStore, MobilePermission, MobileProjectAccess};
use tungstenite::Message;
#[path = "support/mobile.rs"]
mod wire;

/// 接通提供当前项目的有界概况；明确换项目后替换旧概况，且不执行任务。
///
/// 数据构造：同一手机获 A/B 两项目 control；A 有 17 张 Agent 卡，B 有 1 张。
///   首包最多 16 张，所以 A 应省略 17 - 16 = 1 张；任务标题 241 字符超过 240 摘要上限。
///   卡片带 session/run 身份作为负样本，不应发给语音模型；配对 token 同样不能进入提示。
///   A 另有 33 个未开卡 Agent，初始名单上限沿用桌面 32，所以省略 33-32=1 个；第2个已禁用。
/// 执行过程：绑定 A → 读取 session.update → 将其作为同媒体会话的旧配置 → 明确绑定 B。
/// 预期结果：A 含项目、Agent、任务摘要与采样时间；B 含新概况且不含旧概况；两次均无写命令。
#[test]
fn binding_supplies_bounded_context_and_replaces_the_previous_project() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let a = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "项目甲")
        .expect("项目 A")
        .workspace_id;
    std::fs::create_dir(directory.path().join("b")).expect("项目 B 目录");
    let b = store
        .workspace_identity_store()
        .resolve_or_create(&directory.path().join("b"), "项目乙")
        .expect("项目 B")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "手机",
            &"a".repeat(64),
            &[
                MobileProjectAccess {
                    workspace_id: a.clone(),
                    access: MobilePermission::Control,
                },
                MobileProjectAccess {
                    workspace_id: b.clone(),
                    access: MobilePermission::Control,
                },
            ],
        )
        .expect("两项目控制授权");
    let (snapshots, requests) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let (commands, writes) = async_channel::bounded::<LiveCommandRequest>(8);
    let first_project = a.clone();
    let frontend = std::thread::spawn(move || {
        while let Ok(request) = requests.recv_blocking() {
            assert!(
                request.include_cards,
                "开聊概况必须包含已有 Agent 与任务摘要"
            );
            assert_eq!(request.projects.len(), 1, "不能预取另一已授权项目的内容");
            let mut project = request.projects[0].clone();
            let first = project.workspace_id == first_project;
            let count = if first { 17 } else { 1 };
            project.state = "open".into();
            project.route_id = Some("route-current".into());
            project.card_count = Some(count);
            project.cards = Some((1..=count).map(|index| {
                serde_json::from_value::<CardSnapshot>(json!({
                    "node_id":index,"profile":"Codex","title":if first {format!("开发甲{index:02}")} else {"开发乙".into()},
                    "mobile_control":true,"state":"running_tool","run_id":"private-run","session_id":"private-session",
                    "task_title":if first {"修".repeat(240)+"尾"} else {"修复设置页面".into()},
                    "task_summary":if first {"正在检查甲的测试"} else {"正在检查乙的输入"}
                })).expect("真实卡片 DTO")
            }).collect());
            if let Some(reply) = request.agents {
                let roster = (1..=if first { 33 } else { 1 })
                    .map(|index| agent::AgentRosterEntry {
                        definition_id: index,
                        name: if first {
                            format!("未开卡甲{index:02}")
                        } else {
                            "未开卡乙".into()
                        },
                        speech_alias: (first && index == 1).then(|| "小甲".into()),
                        profile: norma::protocol::AgentProfile::Codex,
                        enabled: index != 2,
                        active_node_id: None,
                        busy: false,
                        tracking: agent::AgentTrackingMode::Managed,
                        task_state: agent::AgentTaskState::Ready,
                    })
                    .collect();
                let _ = reply.send(roster);
            }
            let _ = request.reply.send(vec![project]);
        }
    });
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS 身份");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("本机地址")],
        Some(snapshots),
        Some(commands),
    )
    .expect("真实手机 TLS 服务");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("模拟实时上游");
    server
        .use_realtime_server_for_e2e(listener.local_addr().expect("上游地址"))
        .expect("隔离接线");
    let upstream = std::thread::spawn(move || {
        listener.set_nonblocking(true).expect("有界连接等待");
        let mut session = json!({"type":"realtime","instructions":"用中文交谈"});
        let mut updates = Vec::new();
        for _ in 0..2 {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "有效绑定没有连接上游");
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(error) => panic!("上游接入失败: {error}"),
                }
            };
            // 5s = 2s 授权快照预算 + 3s 协议传输余量。
            stream.set_nonblocking(false).expect("阻塞读取 WS 握手");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("有界读取");
            let mut socket = tungstenite::accept(stream).expect("上游 WS");
            socket
                .send(Message::text(
                    json!({"type":"session.created","session":session}).to_string(),
                ))
                .expect("初始配置");
            let update: Value =
                serde_json::from_str(socket.read().expect("Mac 配置").to_text().expect("文本"))
                    .expect("JSON");
            session = update["session"].clone();
            updates.push(session.clone());
            socket
                .send(Message::text(
                    json!({"type":"session.updated","session":session}).to_string(),
                ))
                .expect("确认配置");
        }
        updates
    });
    let mut observed = Vec::new();
    for (index, project) in [a, b].into_iter().enumerate() {
        let mut phone = wire::connect(server.addresses()[0], cert.clone());
        let mut reply = wire::exchange(
            &mut phone,
            &machine,
            "bind",
            json!({
                "type":"bind_realtime","device_id":"phone-a","token":"a".repeat(64),
                "binding":{"call_id":"rtc_context","workspace_id":project,"generation":index+1}
            }),
        );
        // 最多 4 × 2s 心跳足以覆盖 5s 配置期限，避免绑定失败时测试挂起。
        for _ in 0..4 {
            if reply["result"]["state"] != "connecting" {
                break;
            }
            reply = serde_json::from_str(phone.read().expect("绑定结果").to_text().expect("文本"))
                .expect("JSON");
        }
        observed.push(reply["result"]["snapshot_observed_at_unix_ms"].as_u64());
    }
    let updates = upstream.join().expect("取得两次上游配置");
    assert!(
        writes.try_recv().is_err(),
        "开聊与切目标只读概况，不得生成任何任务命令"
    );
    server.shutdown();
    drop(server);
    frontend.join().expect("快照线程结束");
    let initial = updates[0]["instructions"].as_str().expect("首次提示");
    let switched = updates[1]["instructions"].as_str().expect("切换提示");
    assert!(
        initial.contains("未开卡甲01") && initial.contains("小甲"),
        "初始上下文不能遗漏未开卡 Agent 或其语音称呼"
    );
    assert!(
        initial.contains("未开卡甲32"),
        "第32个定义仍应在桌面一致的名单预算中"
    );
    assert!(
        !initial.contains("未开卡甲33"),
        "名单不能突破32个定义的初始预算"
    );
    assert!(
        initial.contains("\"enabled\":false"),
        "不能将禁用 Agent 伪装为可派工"
    );
    assert!(
        switched.contains("未开卡乙"),
        "切目标应提供新项目的未开卡 Agent"
    );
    assert!(
        !switched.contains("未开卡甲"),
        "旧项目名单不能残留在新绑定中"
    );
    let first_time = observed[0].expect("手机必须收到甲的真实快照时间");
    let second_time = observed[1].expect("手机必须收到乙的真实快照时间");
    assert!(
        first_time > 0 && second_time > 0,
        "缺省哨兵不能显示为有效时间"
    );
    assert!(
        initial.contains(&first_time.to_string()),
        "手机和模型必须观察同一份甲概况的时间"
    );
    assert!(
        switched.contains(&second_time.to_string()),
        "换项目后必须显示乙概况的时间"
    );
    assert!(initial.contains("项目甲"), "首次接通缺少绑定项目概况");
    assert!(
        initial.contains("开发甲01"),
        "已有 Agent 名单必须提供给实时模型"
    );
    assert!(
        initial.contains("正在检查甲的测试"),
        "已有任务摘要必须提供给实时模型"
    );
    assert!(
        initial.contains("observed_at_unix_ms"),
        "概况必须标记采样时间，不能冒充持续实时进度"
    );
    assert!(initial.contains("开发甲16"), "预算内第 16 张卡应保留");
    assert!(
        !initial.contains("开发甲17"),
        "第 17 张卡不得突破开聊摘要预算"
    );
    assert!(
        !initial.contains('尾'),
        "超过 240 字符的任务正文不能整段注入"
    );
    assert!(
        !initial.contains("private-session"),
        "模型不需要内部会话身份"
    );
    assert!(!initial.contains("private-run"), "模型不需要内部运行身份");
    assert!(
        !initial.contains(&"a".repeat(64)),
        "配对密钥绝不能注入模型上下文"
    );
    assert!(
        switched.contains("项目乙") && switched.contains("开发乙"),
        "新绑定必须携带新项目和 Agent"
    );
    assert!(
        !switched.contains("项目甲") && !switched.contains("开发甲"),
        "同媒体换项目必须清除旧概况"
    );
    assert!(
        switched.starts_with("用中文交谈"),
        "绑定更新不得丢失供应商原始会话指令"
    );
}
