use super::*;
use norma::agent_hook::{parse_hook_event, HookApprovalRequest, HookSource};

pub(super) fn runtime_with_card() -> (Runtime, NodeId) {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_agent_hook_config(Some(crate::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "test-token".into(),
        workspace_id: "workspace-a".into(),
        workspace_route_id: "route-a".into(),
        norma_socket: "/tmp/norma-test.sock".into(),
    }));
    let id = runtime
        .create_mobile_agent_card(
            AgentProfile::Claude,
            None,
            None,
            canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("应可建卡");
    (runtime, id)
}

/// iOS 创建 Cursor 卡片时应启动现有 cursor-agent PTY，并注入同一卡片的 Hook 身份。
///
/// 数据构造：Cursor definition_id=10、名称 Chloe、640×400 卡片、80×24 终端；PTY factory 只记录命令不启动真实 Provider。
/// 执行过程：配置 Hook ingress → 创建并启动 Cursor 卡片 → 读取卡片快照与捕获命令。
/// 预期结果：卡片为可控 Cursor 且保留定义；命令使用 cursor-agent 并注入 node/profile；不得误启动 Codex。
#[test]
fn mobile_cursor_card_launches_existing_cli_with_bound_hook_identity() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_agent_hook_config(Some(crate::AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:12345/agent-hooks".into(),
        token: "cursor-token".into(),
        workspace_id: "workspace-cursor".into(),
        workspace_route_id: "route-cursor".into(),
        norma_socket: "/tmp/norma-cursor.sock".into(),
    }));
    let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = commands.clone();
    runtime.set_pty_factory(Box::new(move |command, _, _, tx| {
        captured.lock().expect("Cursor 命令记录锁").push(command);
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));

    let node = runtime
        .spawn_mobile_agent_card_for_definition(
            AgentProfile::Cursor,
            10,
            "Chloe".into(),
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("iOS 应能创建 Cursor 卡片");
    let card = runtime
        .mobile_card_snapshots()
        .into_iter()
        .find(|card| card.node_id == node)
        .expect("创建后应有 Cursor 快照");
    let commands = commands.lock().expect("读取 Cursor 启动命令");

    assert_eq!(card.profile, "Cursor", "快照必须保留 Cursor Provider");
    assert!(
        card.mobile_control,
        "iOS 创建的 Cursor 卡片必须可由手机继续控制"
    );
    assert!(
        runtime.live.contains_key(&node),
        "Cursor 必须复用真实卡片 PTY"
    );
    assert_eq!(commands.len(), 1, "创建一张 Cursor 卡只能启动一个 PTY");
    assert!(
        commands[0].args[1].contains("exec cursor-agent"),
        "Cursor 卡片必须启动 cursor-agent，实际命令：{:?}",
        commands[0].args
    );
    assert!(
        commands[0]
            .env
            .iter()
            .any(|(key, value)| key == "MULTISOUL_AGENT_PROFILE" && value == "cursor"),
        "Cursor PTY 必须注入准确 Hook Provider，实际环境：{:?}",
        commands[0].env
    );
    assert!(
        !commands[0].args[1].contains("codex"),
        "Cursor 卡片不得误启动 Codex"
    );
}

/// 所属进程在 kill_all 后立即退出，受控 host 仍必须被回收，不能依赖后台线程来得及调度。
///
/// 数据构造：子测试进程持有一个真实 PTY sleep(60s)，60s 保证观察期内不会自行退出；父进程以 2s 检查回收。
/// 执行过程：子进程把 host PID 写入临时文件 → Runtime 接管 → kill_all → 立即 exit → 父进程检查 PID。
/// 预期结果：测试子进程成功退出，host 不再存在；不能留下停止态或仍运行的孤儿。
#[test]
#[cfg(target_os = "macos")]
fn controlled_host_is_killed_before_owner_process_exits() {
    use terminal::PtyBackend;
    if let Some(path) = std::env::var_os("NORMA_TEST_HOST_PID_FILE") {
        let (mut runtime, id) = runtime_with_card();
        let launch = runtime
            .mobile_agent_launch(id, Path::new("unused"), 80, 24)
            .expect("应预约");
        let (tx, _rx) = crossbeam_channel::unbounded();
        let pty = launch
            .control
            .spawn_pty(
                PtyCommand::new("/bin/sh").args(["-c", "exec sleep 60"]),
                80,
                24,
                tx,
            )
            .expect("Runtime 应启动并接管独立 host");
        let pid = pty.child_pid().expect("应有 host PID");
        std::fs::write(path, pid.to_string()).expect("应记录测试 PID");
        runtime.kill_all();
        std::process::exit(0);
    }
    let directory = tempfile::tempdir().expect("应创建临时目录");
    let path = directory.path().join("host.pid");
    let status = std::process::Command::new(std::env::current_exe().expect("测试程序应存在"))
        .args(["--exact","runtime::mobile_cards::launch_tests::controlled_host_is_killed_before_owner_process_exits"])
        .env("NORMA_TEST_HOST_PID_FILE",&path).status().expect("应启动子测试");
    assert!(status.success(), "拥有者应主动正常退出");
    let pid: i32 = std::fs::read_to_string(path)
        .expect("应得到测试 PID")
        .parse()
        .expect("PID 应为整数");
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let gone = unsafe { libc::kill(pid, 0) } != 0;
    if !gone {
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
    assert!(
        gone,
        "退出后不能留下该次测试 host；后台取消标志不足以保证退出回收"
    );
}

fn approval(node_id: NodeId, run_id: &str, session_id: &str) -> HookApproval {
    HookApproval::new(HookApprovalRequest { run_id: run_id.into(), event: parse_hook_event(HookSource::Claude, node_id,
        serde_json::json!({"hook_event_name":"PermissionRequest", "session_id":session_id, "prompt_id":"turn-a", "tool_name":"Bash"}))
        .expect("应可解析") }, Instant::now()).expect("应可构造审批")
}

/// 受控 Claude 保留原生终端并按准确运行实例审批；关闭后即使有克隆句柄也不得批准。
///
/// 数据构造：一张 Claude 卡，80×24 标准终端；echo backend 仅用于生命周期断言，无模型调用。
/// 执行过程：请求启动 → 生成 CLI 命令 → 注入终端 → 错误会话/实例被拒绝 → 接受匹配请求 → kill_all。
/// 预期结果：default 权限模式及独立 Hook 存在，无 bypass；只接受精确身份，关窗撤销待确认并结束任务。
#[test]
fn controlled_launch_binds_native_terminal_and_cancels_permissions_on_close() {
    let (mut runtime, node_id) = runtime_with_card();
    let launch = runtime
        .mobile_agent_launch(node_id, Path::new("/tmp/hook binary"), 80, 24)
        .expect("应可请求启动");
    assert_eq!(runtime.live_len(), 0, "准备阶段不得运行 Agent");
    let command = launch.command("session-a", "run-a");
    assert!(
        command.args[1].contains("--permission-mode default"),
        "手机 Claude 必须保留原生审批"
    );
    assert!(
        command.args[1].contains("--request-approval"),
        "必须添加本次会话的双向 Hook"
    );
    assert!(
        !command.args[1].contains("dangerously"),
        "受控路径不得包含 bypass"
    );
    assert!(
        !command.args[1].contains("--settings-sources"),
        "不得禁用用户或管理策略"
    );
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = crate::nodes::term::TermLive::spawn_with_factory(
        launch.generation,
        command,
        80,
        24,
        &factory,
    )
    .expect("应可注入终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("应可附着");
    assert_eq!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-a"),
        "绑定必须在 Hook 到达前确定"
    );
    assert_eq!(
        runtime.agent_session_bindings.session_id(node_id),
        None,
        "未发送消息的临时会话不能成为可恢复的持久化绑定"
    );
    let prompt = parse_hook_event(HookSource::Claude, node_id, serde_json::json!({
        "hook_event_name":"UserPromptSubmit", "session_id":"session-a", "prompt_id":"turn-a", "_norma_control_run_id":"run-a"
    })).expect("应可构造真实首条消息事件");
    assert!(
        runtime.observe_mobile_hook(&prompt),
        "当前运行的首条消息应开始回合"
    );
    assert!(
        runtime.apply_agent_task_update(crate::AgentTaskUpdate {
            node_id,
            profile: AgentProfile::Claude,
            provider_event: "UserPromptSubmit".into(),
            to: crate::AgentTaskState::Thinking,
            task_title: None,
            session_id: Some("session-a".into()),
        }),
        "真实消息应推进状态"
    );
    assert_eq!(
        runtime.agent_session_bindings.session_id(node_id),
        Some("session-a"),
        "真实消息后才保存可恢复绑定"
    );
    assert!(
        !runtime.apply_agent_task_update(crate::AgentTaskUpdate {
            node_id,
            profile: AgentProfile::Claude,
            provider_event: "SessionStart".into(),
            to: crate::AgentTaskState::Ready,
            task_title: None,
            session_id: Some("foreign-session".into()),
        }),
        "其他来源的状态更新不得覆盖受控会话绑定"
    );
    assert_eq!(
        runtime.agent_session_id(node_id).as_deref(),
        Some("session-a"),
        "错误 Hook 后仍必须绑定原会话"
    );
    let old_hook = parse_hook_event(HookSource::Claude, node_id, serde_json::json!({
        "hook_event_name":"SessionStart", "session_id":"session-a", "_norma_control_run_id":"run-old"
    })).expect("应可构造旧运行 Hook");
    assert!(
        !runtime.accepts_mobile_hook(&old_hook),
        "相同 Provider session 的旧运行 Hook 也必须拒绝"
    );
    let wrong_session = approval(node_id, "run-a", "session-b");
    assert!(
        !runtime.accept_mobile_approval(wrong_session.clone()),
        "不得接受其他会话"
    );
    assert!(
        !wrong_session.decide(ApprovalDecision::Allow, Instant::now()),
        "被拒绝的句柄必须撤销"
    );
    let wrong_run = approval(node_id, "run-b", "session-a");
    assert!(
        !runtime.accept_mobile_approval(wrong_run),
        "旧运行实例不能命中新终端"
    );
    let audit_directory = tempfile::tempdir().expect("审计目录");
    runtime.mobile_runs.get_mut(&node_id).expect("run").claude = Some(
        claude::ClaudeApprovals::new(storage::ApprovalAuditStore::new(
            audit_directory.path().join("project.db"),
        ))
        .expect("审计准备"),
    );
    let pending = approval(node_id, "run-a", "session-a");
    let mut old_request = pending.request().clone();
    old_request.event.turn_id = Some("turn-old".into());
    let old_request = HookApproval::new(old_request, Instant::now()).expect("应可构造前一回合请求");
    assert!(
        !runtime.accept_mobile_approval(old_request.clone()),
        "同一会话前一回合的请求必须拒绝"
    );
    assert!(
        !old_request.decide(ApprovalDecision::Allow, Instant::now()),
        "前一回合的请求不得再批准"
    );
    assert!(
        runtime.accept_mobile_approval(pending.clone()),
        "匹配的请求应进入待决策"
    );
    assert_eq!(
        runtime.mobile_approvals(node_id).len(),
        1,
        "只能看到一个有效请求"
    );
    runtime.kill_all();
    assert_eq!(
        runtime.agent_task_state(node_id),
        Some(crate::AgentTaskState::Ended),
        "关闭执行器后不能仍显示 Thinking 或 WaitingApproval"
    );
    assert!(
        !pending.decide(ApprovalDecision::Allow, Instant::now()),
        "关窗应撤销所有克隆句柄"
    );
    assert!(
        runtime.mobile_approvals(node_id).is_empty(),
        "关窗不能显示待审批"
    );
}

/// 后台准备结果不得在卡片删除或工作区切换后复活；同一卡片不得双重启动。
///
/// 数据构造：一张待准备卡片、一次重复启动与一次被删除后的准备结果；80×24 是默认 PTY 尺寸。
/// 执行过程：领取启动 → 重复领取 → 删除卡片 → 尝试附着已完成的后台结果。
/// 预期结果：重复与迟到附着失败，卡片与 live 均不复活；终端对象释放走既有 kill 生命周期。
#[test]
fn late_background_launch_does_not_resurrect_deleted_card() {
    let (mut runtime, node_id) = runtime_with_card();
    let launch = runtime
        .mobile_agent_launch(node_id, Path::new("/tmp/hook"), 80, 24)
        .expect("首次领取应成功");
    assert!(
        runtime
            .mobile_agent_launch(node_id, Path::new("/tmp/hook"), 80, 24)
            .is_err(),
        "准备中不得重复启动"
    );
    runtime.remove_node(node_id);
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = crate::nodes::term::TermLive::spawn_with_factory(
        launch.generation,
        launch.command("session-a", "run-a"),
        80,
        24,
        &factory,
    )
    .expect("应可构造晚到结果");
    assert!(
        runtime
            .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
            .is_err(),
        "删除后不得附着"
    );
    assert!(runtime.document.node(node_id).is_none(), "卡片不能复活");
    assert_eq!(runtime.live_len(), 0, "晚到结果不得成为执行器");
}

/// 相同 Workspace 快照重新载入后，旧后台结果不能占用新一轮启动 reservation。
///
/// 数据构造：相同 node/config/snapshot 两次启动，只有后台领取身份必须不同；终端尺寸均为 80×24。
/// 执行过程：领取旧启动 → 恢复相同快照 → 领取新启动 → 旧错误回调取消旧领取。
/// 预期结果：两代身份不同；旧回调不能取消新领取，第三次启动仍应被“准备中”拒绝。
#[test]
fn snapshot_restore_does_not_reuse_background_launch_generation() {
    let (mut runtime, node_id) = runtime_with_card();
    let snapshot = runtime.collect_canvas_snapshot();
    let old = runtime
        .mobile_agent_launch(node_id, Path::new("/tmp/hook"), 80, 24)
        .expect("旧领取应成功");
    runtime.restore_canvas_snapshot(snapshot);
    let new = runtime
        .mobile_agent_launch(node_id, Path::new("/tmp/hook"), 80, 24)
        .expect("恢复后应可新领取");
    assert_ne!(
        old.generation(),
        new.generation(),
        "后台领取身份不能随终端编号一起重置"
    );
    runtime.cancel_mobile_agent_launch(node_id, old.generation());
    assert!(
        runtime
            .mobile_agent_launch(node_id, Path::new("/tmp/hook"), 80, 24)
            .is_err(),
        "旧取消不得释放新的 reservation"
    );
}
