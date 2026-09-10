use super::*;
use std::os::unix::net::UnixListener;
use tungstenite::Message;

pub(in crate::runtime::mobile_cards) fn setup() -> (
    Runtime,
    NodeId,
    tempfile::TempDir,
    std::thread::JoinHandle<Vec<Value>>,
) {
    let directory = tempfile::tempdir().expect("应创建私有测试目录");
    let path = directory.path().join("control.sock");
    let listener = UnixListener::bind(&path).expect("应绑定私有 socket");
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("应连接测试 Provider");
        // 本地 5s 防挂上限，不依赖真实模型或审批 TTL。
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("应设置防挂超时");
        let mut ws = tungstenite::accept(stream).expect("应完成 WebSocket 握手");
        let mut decisions = Vec::<Value>::new();
        let mut reject_interrupt = false;
        while let Ok(Message::Text(text)) = ws.read() {
            let value: Value = serde_json::from_str(&text).expect("应收到有效 JSON");
            match value["method"].as_str() {
                Some("initialize") => ws
                    .send(Message::Text(
                        json!({"id":value["id"],"result":{}}).to_string().into(),
                    ))
                    .expect("应初始化"),
                Some("initialized") => {}
                Some("turn/start") => {
                    decisions.push(value.clone());
                    if value["params"]["input"][0]["text"] == "no-confirm" {
                        continue;
                    }
                    ws.send(Message::Text(json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}).to_string().into())).expect("真实开始事件");
                    ws.send(Message::Text(
                        json!({"id":value["id"],"result":{"turn":{"id":"turn-b"}}})
                            .to_string()
                            .into(),
                    ))
                    .expect("准确RPC回执");
                }
                Some("test/advance") => {
                    for event in value["params"].as_array().expect("应传事件列表") {
                        if event["method"] == "test/rejectInterrupt" {
                            reject_interrupt = true;
                            continue;
                        }
                        ws.send(Message::Text(event.to_string().into()))
                            .expect("应发送 Provider 事件");
                    }
                    ws.send(Message::Text(
                        json!({"id":value["id"],"result":decisions})
                            .to_string()
                            .into(),
                    ))
                    .expect("应发送栅栏回执");
                }
                Some("turn/interrupt") => {
                    assert_eq!(
                        value["params"],
                        json!({"threadId":"thread-a","turnId":"turn-a"}),
                        "中断必须准确绑定线程及回合"
                    );
                    let reply = if reject_interrupt {
                        json!({"id":value["id"],"error":{"code":-32000,"message":"test interrupt rejected"}})
                    } else {
                        json!({"id":value["id"],"result":{}})
                    };
                    ws.send(Message::Text(reply.to_string().into()))
                        .expect("应回执中断指令");
                }
                None => decisions.push(value),
                _ => panic!("测试不应收到其他请求"),
            }
        }
        decisions
    });
    let mut runtime = Runtime::new_for_workspace(directory.path());
    runtime.set_agent_hook_config(Some(crate::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test-token".into(),
        workspace_id: "workspace-a".into(),
        workspace_route_id: "route-a".into(),
        norma_socket: "/tmp/unused.sock".into(),
    }));
    let id = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("应创建空闲卡片");
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 80, 24)
        .expect("应预约启动");
    let control = launch.control.clone();
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        PtyCommand::new("unused"),
        80,
        24,
        &factory,
    )
    .expect("应注入终端后端");
    runtime
        .attach_mobile_live(launch, "thread-a".into(), "run-a".into(), live)
        .expect("应附着正式 Runtime");
    runtime.mobile_runs.get_mut(&id).expect("运行实例").codex = Some(CodexHost {
        task_clock: super::super::timing::TaskClock::default(),
        task_summary: super::super::summary::TaskSummary::default(),
        connection: CodexConnection::connect(&path).expect("应使用正式连接"),
        interrupt: None,
        external_approvals: hooks::ExternalApprovals::default(),
        thread_id: "thread-a".into(),
        turn_id: Some("turn-a".into()),
        initial_turn_id: Some("turn-a".into()),
        latest_turn_id: Some("turn-a".into()),
        approvals: CodexApprovals::new("run-a".into(), "thread-a".into(), Some("turn-a".into())),
        control,
        audit: audit::CodexAudit::new(
            id,
            storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite")),
        )
        .expect("应创建正式后台审计"),
    });
    (runtime, id, directory, server)
}

pub(in crate::runtime::mobile_cards) fn advance(
    runtime: &Runtime,
    id: NodeId,
    events: Vec<Value>,
) -> Value {
    runtime.mobile_runs[&id]
        .codex
        .as_ref()
        .expect("host")
        .connection
        .request("test/advance", json!(events))
        .expect("应请求栅栏")
        .recv_timeout(Duration::from_secs(5))
        .expect("应收栅栏")
        .expect("栅栏成功")
}

pub(super) fn request(id: Value) -> Value {
    json!({"id":id,"method":"item/commandExecution/requestApproval","params":{
        "threadId":"thread-a","turnId":"turn-a","itemId":"item-a","command":"echo hello",
        "availableDecisions":["accept","decline"]
    }})
}

/// Runtime 只回复当前请求一次，桌面先处理及过期不能被旧手机卡片重新批准。
///
/// 数据构造：同一运行的 3 条审批，原始 ID 分别为数字 7、字符串 desktop 和数字 9；120s 为协议 TTL。
/// 执行过程：正式 WS 发请求 → 错 run 被拒绝 → 精确拒绝一次 → 桌面先 resolved → 本地过期 → 回合完成。
/// 预期结果：线上只收到 request 7 的 decline；resolved/expired 后不再卡在 WaitingApproval，Done 不被迟到事件覆盖。
#[test]
fn runtime_codex_decisions_follow_provider_lifecycle() {
    let (mut runtime, id, _directory, server) = setup();
    advance(&runtime, id, vec![request(json!(7))]);
    let pending = runtime.mobile_codex_approvals(id).expect("应读审批");
    assert_eq!(pending.len(), 1, "应只存在当前审批");
    assert!(
        matches!(
            runtime.agent_task_state(id),
            Some(AgentTaskState::WaitingApproval { .. })
        ),
        "待审批应显示等待状态"
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-old", &pending[0].id, ApprovalDecision::Allow)
            .is_err(),
        "旧运行不能批准新请求"
    );
    runtime
        .decide_mobile_codex_approval(id, "run-a", &pending[0].id, ApprovalDecision::Deny)
        .expect("应拒绝当前请求");
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-a", &pending[0].id, ApprovalDecision::Allow)
            .is_err(),
        "重复决定不得覆盖拒绝"
    );
    wait_for_audit(&mut runtime, id, &_directory, "submitted");
    let sent = advance(
        &runtime,
        id,
        vec![
            json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":7}}),
        ],
    );
    assert_eq!(
        sent,
        json!([{"id":7,"result":{"decision":"decline"}}]),
        "只能回传一次原始 ID 和单次拒绝"
    );
    assert!(
        runtime
            .mobile_codex_approvals(id)
            .expect("应读状态")
            .is_empty(),
        "Provider 已解决的请求不得继续显示"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Thinking),
        "请求结束必须离开 WaitingApproval，但不能伪造工具执行"
    );
    wait_for_audit(&mut runtime, id, &_directory, "resolved");
    let changes = storage::ApprovalAuditStore::new(_directory.path().join("audit.sqlite"))
        .list_run("run-a", 0, 256)
        .expect("应读持久化审计");
    assert_eq!(
        changes[1].payload["state"],
        json!({"chosen":"deny"}),
        "先记录尚未发送的决定"
    );
    assert_eq!(
        changes[2].payload["state"],
        json!({"submitted":"deny"}),
        "本地提交必须区别于 Provider 解决"
    );
    assert_eq!(
        changes[3].payload["state"],
        json!("resolved"),
        "Provider 解决不等于已执行"
    );

    advance(&runtime, id, vec![request(json!("desktop"))]);
    let desktop = runtime
        .mobile_codex_approvals(id)
        .expect("桌面请求")
        .remove(0);
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":"desktop"}}),
        ],
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-a", &desktop.id, ApprovalDecision::Allow)
            .is_err(),
        "决定前须消费桌面已解决事件"
    );

    advance(&runtime, id, vec![request(json!(9))]);
    let expired = runtime
        .mobile_codex_approvals(id)
        .expect("过期测试请求")
        .remove(0);
    // 推进域层时钟 = 现在 + 120s TTL；不实际等待两分钟，也不改变生产超时。
    runtime
        .mobile_runs
        .get_mut(&id)
        .expect("run")
        .codex
        .as_mut()
        .expect("host")
        .approvals
        .pending(Instant::now() + norma::agent_hook::APPROVAL_TIMEOUT)
        .expect("应过期");
    assert!(
        runtime
            .mobile_codex_approvals(id)
            .expect("应读状态")
            .is_empty(),
        "过期不得重新变成待审批"
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-a", &expired.id, ApprovalDecision::Allow)
            .is_err(),
        "过期请求不能批准"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Thinking),
        "过期后不能保留手机可处理的等待状态"
    );
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
            request(json!(10)),
            json!({"method":"item/started","params":{"threadId":"thread-a","item":{"type":"commandExecution"}}}),
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"status":"interrupted"}}}),
        ],
    );
    assert!(
        runtime
            .mobile_codex_approvals(id)
            .expect("应读终态")
            .is_empty(),
        "结束后不能重收本回合审批"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Done),
        "迟到请求不得覆盖真实 Done"
    );
    assert_eq!(
        advance(&runtime, id, vec![]),
        sent,
        "错误运行、桌面先处理、过期和晚到均不得多发响应"
    );
    // turn-b 被用户中断；晚到的工具通知不能恢复，只有 turn-c 的真实开始才可恢复。
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-b","status":"interrupted"}}}),
            json!({"method":"item/started","params":{"threadId":"thread-a","turnId":"turn-b","item":{"type":"commandExecution"}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert!(
        runtime.mobile_card_snapshots()[0]
            .task_timing
            .expect("已观察到turn-b开始")
            .completed_elapsed_ms
            .is_some(),
        "准确中断须冻结耗时，晚到工具不能重开计时"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Ended),
        "中断后的迟到工具事件不能复活状态"
    );
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-c"}}}),
        ],
    );
    runtime.drain_mobile_codex();
    assert_eq!(
        runtime.agent_task_state(id),
        Some(AgentTaskState::Thinking),
        "真实的新回合必须能离开前次 Ended"
    );
    assert!(
        runtime.mobile_card_snapshots()[0]
            .task_timing
            .expect("turn-c起点")
            .completed_elapsed_ms
            .is_none(),
        "新回合不能继承turn-b完成耗时"
    );
    runtime.kill_all();
    assert!(
        runtime
            .decide_mobile_codex_approval(id, "run-a", &expired.id, ApprovalDecision::Allow)
            .is_err(),
        "关闭后不能提交决定"
    );
    server.join().expect("测试服务端应正常关闭");
}

pub(super) fn wait_for_audit(
    runtime: &mut Runtime,
    id: NodeId,
    directory: &tempfile::TempDir,
    phase: &str,
) {
    let store = storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite"));
    // 5s 是本地 SQLite/线程防挂上限；每 5ms 仅驱动正式 Runtime 和读取测试数据库。
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.drain_mobile_codex();
        if store
            .list_run("run-a", 0, 256)
            .expect("应读取已提交记录")
            .iter()
            .any(|e| e.payload["state"] == phase || e.payload["state"].get(phase).is_some())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "节点 {id} 应持久化阶段 {phase}，不能伪造完成"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// SQLite 被占用时决定不可提前发送；落盘期间桌面处理后，提交回执也不可使旧请求复活。
///
/// 数据构造：一个 IMMEDIATE 写事务阻止审计提交，request 7 选择 Allow，桌面随后先处理；5s 为本地防挂上限。
/// 执行过程：请求先持久化 → 锁库 → 选择批准 → 核对线上无响应 → 桌面 resolved → 释放锁 → 消费审计回执。
/// 预期结果：Chosen 留在数据库，线上始终没有 accept；Provider resolved 单独记录，不能当作手机成功执行。
#[test]
fn durable_choice_cannot_reanimate_a_desktop_resolved_request() {
    let (mut runtime, id, directory, server) = setup();
    advance(&runtime, id, vec![request(json!(7))]);
    let pending = runtime
        .mobile_codex_approvals(id)
        .expect("待审批")
        .remove(0);
    wait_for_audit(&mut runtime, id, &directory, "pending");
    let lock = rusqlite::Connection::open(directory.path().join("audit.sqlite")).expect("测试连接");
    lock.execute_batch("BEGIN IMMEDIATE").expect("应持有写锁");
    runtime
        .decide_mobile_codex_approval(id, "run-a", &pending.id, ApprovalDecision::Allow)
        .expect("应接收待落盘选择");
    runtime.drain_mobile_codex();
    assert_eq!(
        advance(&runtime, id, vec![]),
        json!([]),
        "数据库提交前不得向 Provider 发送批准"
    );
    advance(
        &runtime,
        id,
        vec![
            json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":7}}),
        ],
    );
    runtime.drain_mobile_codex();
    lock.execute_batch("COMMIT").expect("应释放写锁");
    wait_for_audit(&mut runtime, id, &directory, "resolved");
    assert_eq!(
        advance(&runtime, id, vec![]),
        json!([]),
        "已解决请求不能因迟到落盘回执而再次批准"
    );
    runtime.kill_all();
    assert_eq!(
        server.join().expect("服务端退出"),
        Vec::<Value>::new(),
        "完整连接内不得发送任何决定"
    );
}

/// 审计写入失败时关闭受控通道，不能把内存选择当作已保存或继续批准。
///
/// 数据构造：SQLite 触发器仅在测试数据库拒绝 INSERT；一条真实 WS 请求，1 个 Allow 决定。
/// 执行过程：存入 Pending → 创建拒绝写入触发器 → choose Allow → 驱动 Runtime 消费错误回执。
/// 预期结果：运行变成 Failed，host 消失，线上无批准；数据库仍只有 Pending，没有伪造 Submitted。
#[test]
fn failed_audit_commit_stops_control_without_sending() {
    let (mut runtime, id, directory, server) = setup();
    advance(&runtime, id, vec![request(json!(7))]);
    let pending = runtime
        .mobile_codex_approvals(id)
        .expect("待审批")
        .remove(0);
    wait_for_audit(&mut runtime, id, &directory, "pending");
    let db = rusqlite::Connection::open(directory.path().join("audit.sqlite")).expect("测试连接");
    db.execute_batch("CREATE TRIGGER reject_audit BEFORE INSERT ON approval_audit BEGIN SELECT RAISE(ABORT,'test disk failure'); END").expect("应注入写入故障");
    runtime
        .decide_mobile_codex_approval(id, "run-a", &pending.id, ApprovalDecision::Allow)
        .expect("决定仅在内存排队");
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.mobile_runs.contains_key(&id) && Instant::now() < deadline {
        runtime.drain_mobile_codex();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !runtime.mobile_runs.contains_key(&id),
        "审计不可用时不能保留可决策 host"
    );
    assert!(
        matches!(
            runtime.agent_task_state(id),
            Some(AgentTaskState::Failed { .. })
        ),
        "写失败必须明确可见"
    );
    assert_eq!(
        server.join().expect("服务端应关闭"),
        Vec::<Value>::new(),
        "写失败不得发出批准"
    );
    let rows = storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite"))
        .list_run("run-a", 0, 256)
        .expect("应读取原始记录");
    assert_eq!(rows.len(), 1, "失败事务不能伪造已记录选择");
    assert_eq!(
        rows[0].payload["state"],
        json!("pending"),
        "不能把 Pending 改为 Submitted"
    );
}
