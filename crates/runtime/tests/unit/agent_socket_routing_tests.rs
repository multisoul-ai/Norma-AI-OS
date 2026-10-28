use super::*;

/// 恢复的 Agent Session 必须继承所属 Norma socket，不能在 App 重启后回退到其他实例。
///
/// 数据构造（含关键数值的推导过程）：
///   restored Agent = Henry/Codex/node 45(1)
///   owning socket = /tmp/multisoul-norma-dev.sock(1)
///   foreign socket = /tmp/multisoul-norma.sock(1)
///   saved session = codex-session-henry(1)
///   workspace identity = workspace-alpha + route-alpha-7(2 fields)
///   empty RestoreCtx next_session_id = 1（首个可分配的 PTY session）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用开发版 Workspace 配置构造生产 RestoreCtx。
///   2. RestoreCtx 为 Henry 生成 AgentHookEnv。
///   3. 复用真实 resume command builder 构造恢复命令并检查最终环境。
///
/// 预期结果：
///   - 正断言：Henry 继续 resume 原 Codex session。
///   - 正断言：恢复命令携带开发版 socket、Workspace ID 与 route ID。
///   - 负断言：恢复命令不得回退到正式版 socket。
#[test]
fn restored_agent_session_keeps_the_owning_norma_socket() {
    let config = AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
        token: "tok_henry".to_string(),
        workspace_id: "workspace-alpha".to_string(),
        workspace_route_id: "route-alpha-7".to_string(),
        norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
    };
    let mut live = HashMap::<NodeId, Box<dyn LiveNode>>::new();
    let mut terminal_shells = HashMap::new();
    let agent_session_bindings = AgentSessionBindings::default();
    let mut events = RuntimeEvents::default();
    let mut next_session_id = 1;
    let question_processes = HashMap::from([(45, "restored-process".to_owned())]);
    let restore_ctx = RestoreCtx {
        question_processes: &question_processes,
        pty_factory: None,
        live: &mut live,
        terminal_shells: &mut terminal_shells,
        agent_session_bindings: &agent_session_bindings,
        events: &mut events,
        next_session_id: &mut next_session_id,
        workspace_root: Some(Path::new("/tmp/workspace-alpha")),
        agent_hook_config: Some(&config),
    };
    let node_session = NodeSession {
        node_id: 45,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("codex-session-henry".to_string()),
    };

    let hook_env = restore_ctx
        .make_hook_env(45, AgentProfile::Codex)
        .expect("有编排配置时 Henry 恢复必须生成 AgentHookEnv");
    let command = build_agent_restore_command_for_test(
        &node_session,
        AgentProfile::Codex,
        Some(&hook_env),
        Some(Path::new("/tmp/workspace-alpha")),
    );
    assert!(
        command
            .env
            .iter()
            .any(|(key, value)| key == "NORMA_CONTROL_RUN_ID" && value == "restored-process"),
        "恢复命令必须带spawn前登记的原生进程身份"
    );

    assert!(
        command.args.join(" ").contains("codex-session-henry"),
        "恢复命令必须继续使用 Henry 的持久化 Codex session，实际参数：{:?}",
        command.args
    );
    assert!(
        command.env.iter().any(|(key, value)| {
            key == "NORMA_SOCKET" && value == "/tmp/multisoul-norma-dev.sock"
        }),
        "恢复后的 Henry 必须显式绑定开发版 Norma socket，实际环境：{:?}",
        command.env
    );
    assert!(
        command
            .env
            .iter()
            .any(|(key, value)| key == "NORMA_WORKSPACE_ID" && value == "workspace-alpha"),
        "恢复后的 Henry 必须保留 Workspace ID，实际环境：{:?}",
        command.env
    );
    assert!(
        command
            .env
            .iter()
            .any(|(key, value)| { key == "NORMA_WORKSPACE_ROUTE_ID" && value == "route-alpha-7" }),
        "恢复后的 Henry 必须保留 Workspace route ID，实际环境：{:?}",
        command.env
    );
    assert!(
        command
            .env
            .iter()
            .all(|(key, value)| key != "NORMA_SOCKET" || value != "/tmp/multisoul-norma.sock"),
        "恢复后的开发版 Henry 不得回退到正式版 Norma socket"
    );
}
