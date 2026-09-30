use super::*;
use norma::agent_hook::{
    forward_permission, ApprovalDecision, ApprovalStatus, HookForwardEnv, HookSource,
};
use std::io::Write;
use std::time::Instant;

/// 接收线程必须把继承来的非阻塞 socket 转为带超时的阻塞读取，支持分包 HTTP。
///
/// 数据构造：listener 收到一条只有请求行的 POST；显式打开 socket 非阻塞，模拟 macOS accept 继承。
/// header 延迟 100ms = 4 × CLIENT_RECV_INTERVAL(25ms)，低于服务端读超时 2s。
/// 执行过程：先写请求行 → 交给 handler → 验证没有提早返回 400 → 发送剩余头和 body → 读取事件。
/// 预期结果：分包等待期间 handler 不返回；完整请求只投递 node 7，不丢事件或重复投递。
#[test]
fn fragmented_request_waits_even_when_accepted_socket_is_nonblocking() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("应创建 socket pair");
    let mut client = TcpStream::connect(listener.local_addr().expect("应有地址")).expect("应连接");
    let (server, _) = listener.accept().expect("应接收");
    server.set_nonblocking(true).expect("模拟继承非阻塞标志");
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let (token, events, _, _) = registry
        .lock()
        .expect("应注册")
        .register()
        .expect("应生成 token");
    let event = norma::agent_hook::parse_hook_event(
        HookSource::Claude,
        7,
        serde_json::json!({"hook_event_name":"Stop"}),
    )
    .expect("应解析事件");
    let body = serde_json::to_string(&event).expect("应编码事件");
    client
        .write_all(b"POST /agent-hooks HTTP/1.1\r\n")
        .expect("先只发送请求行");
    let (started, ready) = bounded(1);
    let (done, completion) = bounded(1);
    let (waits, _) = bounded(1);
    let worker = thread::spawn(move || {
        started.send(()).expect("通知 handler 即将开始");
        handle_stream(server, &registry, &waits);
        let _ = done.send(());
    });
    ready
        .recv_timeout(Duration::from_secs(5))
        .expect("handler 应开始");
    assert!(
        completion.recv_timeout(Duration::from_millis(100)).is_err(),
        "未收到完整 HTTP 时不得把 WouldBlock 当坏请求"
    );
    write!(
        client,
        "Authorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("应补全请求");
    completion
        .recv_timeout(Duration::from_secs(5))
        .expect("补全后应完成");
    worker.join().expect("handler 应退出");
    assert_eq!(
        events.try_recv().expect("分包事件不得丢失").node_id,
        7,
        "必须投递原 node 7"
    );
    assert!(events.try_recv().is_err(), "分包不得导致重复投递");
}

/// 旧 bridge 缺失顶层 turn_id 时从原始 Provider JSON 补齐，已有身份不得被覆盖。
///
/// 数据构造：node7 的 UserPromptSubmit，raw prompt_id=raw-turn；旧封包无 turn_id，新封包为envelope-turn。
/// 执行过程：通过真实本机 HTTP socket 解析旧封包 → 解析顶层与 raw 不同的新封包。
/// 预期结果：旧事件恢复 raw-turn，新事件保留 envelope-turn，不生成另一会话或覆盖顶层身份。
#[test]
fn legacy_envelope_recovers_only_missing_turn_identity() {
    fn read(turn: Option<&str>) -> AgentHookEvent {
        let listener = TcpListener::bind("127.0.0.1:0").expect("应创建 socket pair");
        let mut client =
            TcpStream::connect(listener.local_addr().expect("应有地址")).expect("应连接");
        let (server, _) = listener.accept().expect("应接收");
        let mut event = norma::agent_hook::parse_hook_event(HookSource::Claude, 7,
            serde_json::json!({"hook_event_name":"UserPromptSubmit","session_id":"session-a","prompt_id":"raw-turn"}))
            .expect("应解析原生事件");
        event.turn_id = turn.map(str::to_owned);
        let body = serde_json::to_string(&event).expect("应编码");
        write!(client, "POST /agent-hooks HTTP/1.1\r\nAuthorization: Bearer test\r\nContent-Length: {}\r\n\r\n{body}", body.len()).expect("应发送");
        let (_, http::HookRequest::Event(event)) =
            http::read_hook_request(&server).expect("应解析 HTTP")
        else {
            panic!("普通事件不能变成审批");
        };
        event
    }
    let old = read(None);
    assert_eq!(
        old.turn_id.as_deref(),
        Some("raw-turn"),
        "旧 bridge 必须恢复真实回合身份"
    );
    assert_eq!(
        old.session_id.as_deref(),
        Some("session-a"),
        "不得改变原会话"
    );
    let new = read(Some("envelope-turn"));
    assert_eq!(
        new.turn_id.as_deref(),
        Some("envelope-turn"),
        "新 bridge 顶层身份优先"
    );
    assert_ne!(
        new.turn_id.as_deref(),
        Some("raw-turn"),
        "raw JSON 不得覆盖已提供的身份"
    );
}

/// 真实 HTTP 审批等待不得占满普通事件 workers；撤销 Workspace 后待确认请求失效。
///
/// 数据构造：5 个审批 = 4 个普通 worker + 1，若阻塞原 worker 则第 5 个无法入队。
/// 等待上限 5 秒用于本机线程调度防挂；审批业务超时仍为 120 秒。
/// 执行过程：注册工作区 → 启动 listener → 发 5 个审批 → 第一条拒绝 → 注销 → 剩余回退原生确认。
/// 预期结果：5 条全到达；首次拒绝透传且不能改为允许；其余被取消，无授权 JSON。
#[test]
fn permission_wait_does_not_block_event_workers_and_revoke_cancels() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("应可启动测试入口");
    listener.set_nonblocking(true).expect("listener 应为非阻塞");
    let endpoint = format!(
        "http://{}/agent-hooks",
        listener.local_addr().expect("应有地址")
    );
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let (token, _, _, approvals) = registry
        .lock()
        .expect("应可注册")
        .register()
        .expect("安全注册应成功");
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_registry = Arc::clone(&registry);
    let worker_shutdown = Arc::clone(&shutdown);
    let server = thread::spawn(move || accept_loop(listener, worker_registry, worker_shutdown));
    let clients: Vec<_> = (0..5).map(|node_id| {
        let env = HookForwardEnv { node_id, endpoint: endpoint.clone(), token: token.clone() };
        thread::spawn(move || { let result = forward_permission(HookSource::Claude, serde_json::json!({
            "hook_event_name": "PermissionRequest", "session_id": "session-a", "prompt_id": "turn-a", "tool_name": "Bash"
        }), env, "run-a".into());
        if let Err(error) = &result { eprintln!("permission test client {node_id}: {error}"); }
        result })
    }).collect();
    let received: Vec<_> = (0..5)
        .map(|_| {
            approvals
                .recv_timeout(Duration::from_secs(5))
                .expect("待审批不应阻塞普通 worker")
        })
        .collect();
    assert_eq!(
        received.len(),
        5,
        "必须超过原 worker 数量，证明异步等待生效"
    );
    assert!(
        received[0].decide(ApprovalDecision::Deny, Instant::now()),
        "首次拒绝应成功"
    );
    assert!(
        !received[0].decide(ApprovalDecision::Allow, Instant::now()),
        "冲突批准不得覆盖拒绝"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        received[0].status(Instant::now()),
        ApprovalStatus::Responded(_)
    ) && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        received[0].status(Instant::now()),
        ApprovalStatus::Responded(ApprovalDecision::Deny),
        "拒绝应送回 Hook"
    );
    registry.lock().expect("应可注销").unregister(&token);
    let outputs: Vec<_> = clients
        .into_iter()
        .map(|client| {
            client
                .join()
                .expect("client 应正常退出")
                .expect("有效连接应得到响应")
        })
        .collect();
    shutdown.store(true, Ordering::Release);
    server.join().expect("所有后台线程必须收口");
    assert_eq!(
        outputs.iter().filter(|output| output.is_some()).count(),
        1,
        "仅一条请求有拒绝 JSON"
    );
    assert!(
        !outputs
            .iter()
            .flatten()
            .any(|output| output["hookSpecificOutput"]["decision"]["behavior"] == "allow"),
        "不得产生未经批准的 allow"
    );
    assert_eq!(
        received[1].status(Instant::now()),
        ApprovalStatus::Cancelled,
        "注销后第二条必须取消"
    );
    assert!(
        !received[1].decide(ApprovalDecision::Allow, Instant::now()),
        "注销后迟到批准必须失败"
    );
}

/// Hook 进程先断开时，即使已有尚未写出的批准也不得通过 HTTP 送出。
///
/// 数据构造：一对 loopback socket、一个 pending ticket 和一个 allow；等待上限 5s 远小于业务超时 120s。
/// 执行过程：客户端 shutdown → 设置 allow → 将服务端 socket 交给真实等待线程 → 等待取消 → 关闭线程。
/// 预期结果：EOF 撤销未写出的决定；无可消费授权，重复 allow 失败，线程退出完成。
#[test]
fn disconnected_hook_revokes_an_unsent_allow() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("应可创建 socket pair");
    let client = TcpStream::connect(listener.local_addr().expect("应有地址")).expect("应可连接");
    let (server, _) = listener.accept().expect("应接受连接");
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let (token, _, _, _) = registry
        .lock()
        .expect("应可注册")
        .register()
        .expect("token 应安全生成");
    let ticket = norma::agent_hook::HookApproval::new(norma::agent_hook::HookApprovalRequest {
        run_id: "run-a".into(), event: norma::agent_hook::parse_hook_event(HookSource::Claude, 7,
            serde_json::json!({"hook_event_name":"PermissionRequest", "session_id":"session-a", "prompt_id":"turn-a", "tool_name":"Bash"})).expect("应可构造事件")
    }, Instant::now()).expect("应可构造 ticket");
    client
        .shutdown(std::net::Shutdown::Both)
        .expect("模拟 Hook 已退出");
    // shutdown() schedules FIN; wait for peer visibility before testing the EOF branch.
    // Otherwise the sender may legally commit its response before FIN reaches this socket.
    server
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("应设置 EOF 防挂超时");
    assert_eq!(
        server.peek(&mut [0_u8; 1]).expect("应可观察对端关闭"),
        0,
        "服务端必须先观察到 EOF"
    );
    assert!(
        ticket.decide(ApprovalDecision::Allow, Instant::now()),
        "模拟尚未观察到断开的竞争决定"
    );
    let (sender, receiver) = bounded(1);
    sender
        .send(WaitingHook::new(server, token, ticket.clone()).expect("应可创建 waiter"))
        .expect("应交给等待线程");
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_shutdown = Arc::clone(&shutdown);
    let worker = thread::spawn(move || approval_wait::run(receiver, registry, worker_shutdown));
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticket.status(Instant::now()) != ApprovalStatus::Cancelled && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    shutdown.store(true, Ordering::Release);
    worker.join().expect("等待线程必须退出");
    assert_eq!(
        ticket.status(Instant::now()),
        ApprovalStatus::Cancelled,
        "EOF 应撤销 allow"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "断开后的 allow 不能返回给 Provider"
    );
    assert!(
        !ticket.decide(ApprovalDecision::Allow, Instant::now()),
        "迟到的重复批准必须失败"
    );
}
