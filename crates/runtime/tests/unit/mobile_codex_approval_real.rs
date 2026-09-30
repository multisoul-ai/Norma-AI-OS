use super::*;
use crate::NodeView;
use terminal::PtyEvent;

pub(super) fn task(command: &str) -> String {
    format!("This is an I/O test in a disposable directory. Use exec_command exactly once: {command}\nRequest approval for this exact write. Do not run other commands or delegate. If denied, do not retry. Reply briefly and stop.")
}

pub(super) fn approval(runtime: &mut Runtime, id: NodeId, expected_turn: &str) -> CodexApproval {
    // 120s covers model cold startup; the approval itself retains its independent 120s TTL.
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut started = false;
    loop {
        runtime.drain_all();
        started |= runtime.mobile_runs[&id]
            .codex
            .as_ref()
            .expect("host")
            .turn_id
            .as_deref()
            == Some(expected_turn);
        if let Some(request) = runtime
            .mobile_codex_approvals(id)
            .expect("应保持受控连接")
            .into_iter()
            .next()
        {
            return request;
        }
        let state = runtime.agent_task_state(id);
        let terminal = matches!(
            state,
            Some(AgentTaskState::Done | AgentTaskState::Ended | AgentTaskState::Failed { .. })
        );
        if (started && terminal) || Instant::now() >= deadline {
            let screen = runtime
                .node_view(id)
                .map(|view| {
                    let NodeView::Term { snapshot, .. } = view;
                    snapshot
                        .rows
                        .iter()
                        .flatten()
                        .map(|cell| cell.c)
                        .collect::<String>()
                })
                .unwrap_or_default();
            panic!("未收到审批，状态 {state:?}，原生终端：{screen}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn completed(runtime: &mut Runtime, id: NodeId, state: AgentTaskState) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        runtime.drain_all();
        if runtime.agent_task_state(id) == Some(state.clone()) {
            return;
        }
        assert!(
            !matches!(
                runtime.agent_task_state(id),
                Some(AgentTaskState::Failed { .. })
            ),
            "Provider 不应失败：{:?}",
            runtime.agent_task_state(id)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "未收到真实终态 {state:?}，实际 {:?}",
        runtime.agent_task_state(id)
    );
}

pub(super) fn exact_command(approval: &CodexApproval, command: &str) {
    let actual = approval.params["command"]
        .as_str()
        .expect("命令审批必须带命令");
    assert!(
        actual == command
            || actual == format!("/bin/zsh -lc '{command}'")
            || actual == format!("/bin/bash -lc '{command}'"),
        "仅允许精确临时写入，实际命令：{actual}"
    );
}

/// 真实 Codex 的 Runtime 审批先落盘再响应，原生终端并存；拒绝、批准和取消均核对磁盘结果。
///
/// 数据构造：私有 /tmp 项目及 SQLite，first/second/third.txt 三个文件，仅写 hello-from-norma；前三回合拒绝、批准、中断，后两回合由独立客户端先拒绝或在手机提交期间抢先拒绝。
/// 执行过程：隔离 host → 正式 connect/start/attach → Runtime 决策 → 等待真实回合结束 → 分页读取审计 → kill_all。
/// 预期结果：first/third/两竞争文件不存在，second 内容准确；审计区分选择与提交，另一端已拒绝后不得补发批准。
/// 仅本测试 host 按用户授权关闭外部 Hook；正式启动代码及用户配置不变。此处是 Runtime + 真实 CLI，不是真实 .app/iPhone。
#[test]
#[ignore = "requires installed Codex/model; user authorized isolated host without external hooks"]
fn real_codex_runtime_approval_audit_and_cancel() {
    let directory = tempfile::Builder::new()
        .prefix("norma-codex-approval-")
        .tempdir_in("/tmp")
        .expect("应创建隔离目录");
    let mut runtime = Runtime::new_for_workspace(directory.path());
    runtime.set_agent_hook_config(Some(crate::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test-token".into(),
        workspace_id: "isolated-test".into(),
        workspace_route_id: "isolated-route".into(),
        norma_socket: "/tmp/unused.sock".into(),
    }));
    let id = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("应创建空卡");
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 100, 30)
        .expect("应预约");
    let socket = directory.path().join("control.sock");
    let endpoint = format!("unix://{}", socket.display());
    let run_id = fresh_control_id().expect("应生成运行身份");
    let store = storage::ApprovalAuditStore::new(directory.path().join("audit.sqlite"));
    let audit = audit::CodexAudit::new(id, store.clone()).expect("应开正式审计");
    let (output_tx, output_rx) = crossbeam_channel::bounded(64);
    let host_command = PtyCommand::new("/bin/zsh")
        .args([
            "-lic",
            &format!("exec codex --disable hooks app-server --listen {endpoint}"),
        ])
        .cwd(directory.path());
    let _host_process = launch
        .control
        .spawn_pty(host_command, 80, 24, output_tx)
        .expect("应启动隔离 host");
    let output = std::thread::spawn(move || {
        while let Ok(event) = output_rx.recv() {
            if matches!(event, PtyEvent::Exited { .. }) {
                break;
            }
        }
    });
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    let third = directory.path().join("third.txt");
    let first_command = format!("printf hello-from-norma > {}", first.display());
    let host = CodexHost::connect_and_start(
        &launch,
        &socket,
        Some(&task(&first_command)),
        &run_id,
        audit,
    )
    .expect("正式协议初始化及真实任务应成功");
    let session = host.thread_id.clone();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let native = PtyCommand::new("/bin/zsh")
        .args([
            "-lic",
            &format!("exec codex --remote {endpoint} resume {session}"),
        ])
        .cwd(directory.path());
    let pty = launch
        .control
        .spawn_pty(native, 100, 30, sender)
        .expect("应启动原生终端");
    runtime
        .attach_mobile_agent(PreparedMobileAgent {
            launch,
            session_id: session.clone(),
            run_id: run_id.clone(),
            pty,
            receiver,
            codex: Some(host),
            claude: None,
        })
        .expect("应由 Runtime 接管");

    let first_turn = runtime.mobile_runs[&id]
        .codex
        .as_ref()
        .expect("host")
        .turn_id
        .clone()
        .expect("首回合");
    let denied = approval(&mut runtime, id, &first_turn);
    eprintln!(
        "Codex offered decisions: {}",
        denied.params["availableDecisions"]
    );
    exact_command(&denied, &first_command);
    assert!(!first.exists(), "审批前不得写 first");
    runtime
        .decide_mobile_codex_approval(id, &run_id, &denied.id, ApprovalDecision::Deny)
        .expect("应接收拒绝");
    assert!(
        runtime
            .decide_mobile_codex_approval(id, &run_id, &denied.id, ApprovalDecision::Allow)
            .is_err(),
        "同一请求不能改为批准"
    );
    let denied_state = if denied
        .response(ApprovalDecision::Deny)
        .expect("应有拒绝选项")["decision"]
        == "cancel"
    {
        AgentTaskState::Ended
    } else {
        AgentTaskState::Done
    };
    completed(&mut runtime, id, denied_state);
    assert!(!first.exists(), "被拒绝命令绝不能执行");

    let second_command = format!("printf hello-from-norma > {}", second.display());
    let host = runtime.mobile_runs[&id].codex.as_ref().expect("host");
    let turn = CodexHost::rpc(
        &host.connection,
        "turn/start",
        json!({"threadId":session,"input":[{"type":"text","text":task(&second_command)}]}),
    )
    .expect("应开始第二个真实回合");
    let allowed = approval(
        &mut runtime,
        id,
        turn["turn"]["id"].as_str().expect("第二回合"),
    );
    exact_command(&allowed, &second_command);
    assert!(!second.exists(), "批准前不得写 second");
    runtime
        .decide_mobile_codex_approval(id, &run_id, &allowed.id, ApprovalDecision::Allow)
        .expect("应接收单次批准");
    completed(&mut runtime, id, AgentTaskState::Done);
    assert_eq!(
        std::fs::read_to_string(&second).expect("批准后应执行写入"),
        "hello-from-norma",
        "仅执行精确测试写入"
    );
    assert!(!first.exists(), "后续批准不能补执行前次拒绝");

    let third_command = format!("printf hello-from-norma > {}", third.display());
    let host = runtime.mobile_runs[&id].codex.as_ref().expect("host");
    let turn = CodexHost::rpc(
        &host.connection,
        "turn/start",
        json!({"threadId":session,"input":[{"type":"text","text":task(&third_command)}]}),
    )
    .expect("应开始取消回合");
    let cancelled = approval(
        &mut runtime,
        id,
        turn["turn"]["id"].as_str().expect("第三回合"),
    );
    exact_command(&cancelled, &third_command);
    let interrupted = runtime
        .interrupt_mobile_codex(id, &run_id, &cancelled.turn_id)
        .expect("正式手机中断入口应接收指令");
    assert!(
        runtime.mobile_codex_interrupt_pending(id, &run_id),
        "发出指令时尚未收到真实结束"
    );
    completed(&mut runtime, id, AgentTaskState::Ended);
    assert_eq!(
        interrupted
            .try_recv()
            .expect("应有真实中断回执")
            .expect("中断应成功")["status"],
        "interrupted",
        "回执必须等待真实 Provider 中断完成"
    );
    assert!(
        runtime
            .decide_mobile_codex_approval(id, &run_id, &cancelled.id, ApprovalDecision::Allow)
            .is_err(),
        "中断后迟到批准不得发送"
    );
    assert!(!third.exists(), "取消审批不能执行 third");

    // 5s 只等待后台 SQLite 收尾；不把收到 RPC 等同于持久化完成。
    let deadline = Instant::now() + Duration::from_secs(5);
    let rows = loop {
        runtime.drain_all();
        let rows = store.list_run(&run_id, 0, 256).expect("应读回审计");
        if rows.iter().any(|r| {
            r.request_id == cancelled.id
                && matches!(r.payload["state"].as_str(), Some("cancelled" | "resolved"))
        }) {
            break rows;
        }
        assert!(Instant::now() < deadline, "最后请求终态必须落盘");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        rows.iter()
            .any(|r| r.request_id == denied.id && r.payload["state"] == json!({"chosen":"deny"})),
        "拒绝意图必须持久化"
    );
    assert!(
        rows.iter()
            .any(|r| r.request_id == allowed.id
                && r.payload["state"] == json!({"submitted":"allow"})),
        "批准的本地提交必须单独记录"
    );
    assert!(
        rows.iter()
            .any(|r| r.request_id == allowed.id && r.payload["state"] == json!("resolved")),
        "Provider 请求解决必须独立记录"
    );
    assert!(
        !rows
            .iter()
            .any(|r| r.request_id == cancelled.id && r.payload["state"].get("submitted").is_some()),
        "取消请求不能伪造响应已发送"
    );
    competition_real::verify_competition(
        &mut runtime,
        id,
        directory.path(),
        &socket,
        &session,
        &run_id,
        &store,
    );
    runtime.kill_all();
    output.join().expect("关闭后 host 输出消费应结束");
}
