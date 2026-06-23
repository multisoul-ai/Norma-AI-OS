use norma::agent_hook::{
    build_hook_command, forward_hook, hook_events_for_source, install_hook_config,
    parse_hook_event, task_state_for_provider_event, AgentHookTaskState, HookForwardEnv,
    HookForwardOutcome, HookInstallPaths, HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;

/// Claude UserPromptSubmit hook JSON 应归一化为 Thinking task hint。
///
/// 数据构造（含关键数值的推导过程）：
///   source=Claude → profile=AgentProfile::Claude
///   node_id=7 来自 MULTISOUL_NODE_ID
///   raw.hook_event_name="UserPromptSubmit"
///   raw.session_id="s1"，raw.turn_id="t1"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_hook_event(Claude, 7, raw)
///   2. task_state_for_provider_event(event)
///
/// 预期结果：
///   - 正断言：provider_event=UserPromptSubmit，task hint=Thinking
///   - 正断言：session_id/turn_id 被保留
///   - 负断言：不应误归一化为 RunningTool
#[test]
fn test_claude_user_prompt_submit_maps_to_thinking() {
    let raw = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s1",
        "turn_id": "t1"
    });

    let event = parse_hook_event(HookSource::Claude, 7, raw).expect("hook JSON 应可解析");
    let state = task_state_for_provider_event(&event);

    assert_eq!(
        event.profile,
        AgentProfile::Claude,
        "source=Claude 应归一化 profile=Claude"
    );
    assert_eq!(
        event.provider_event, "UserPromptSubmit",
        "provider_event 应读取 hook_event_name"
    );
    assert_eq!(
        state,
        Some(AgentHookTaskState::Thinking),
        "UserPromptSubmit 应映射为 Thinking"
    );
    assert_eq!(
        event.session_id,
        Some("s1".to_string()),
        "应保留 session_id"
    );
    assert_eq!(event.turn_id, Some("t1".to_string()), "应保留 turn_id");
    assert_ne!(
        state,
        Some(AgentHookTaskState::RunningTool { tool_name: None }),
        "UserPromptSubmit 不应映射为 RunningTool"
    );
}

/// Codex PreToolUse hook JSON 应保留 tool_name 并归一化为 RunningTool。
///
/// 数据构造（含关键数值的推导过程）：
///   source=Codex → profile=AgentProfile::Codex
///   node_id=9
///   raw.hook_event_name="PreToolUse"
///   raw.tool_name="Bash"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_hook_event(Codex, 9, raw)
///   2. task_state_for_provider_event(event)
///
/// 预期结果：
///   - 正断言：tool_name=Some("Bash")
///   - 正断言：task hint=RunningTool{Some("Bash")}
///   - 负断言：不应误归一化为 WaitingApproval
#[test]
fn test_codex_pre_tool_use_maps_to_running_tool() {
    let raw = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash"
    });

    let event = parse_hook_event(HookSource::Codex, 9, raw).expect("hook JSON 应可解析");
    let state = task_state_for_provider_event(&event);

    assert_eq!(
        event.profile,
        AgentProfile::Codex,
        "source=Codex 应归一化 profile=Codex"
    );
    assert_eq!(
        event.tool_name,
        Some("Bash".to_string()),
        "应保留 tool_name"
    );
    assert_eq!(
        state,
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("Bash".to_string()),
        }),
        "PreToolUse 应映射为 RunningTool(Bash)"
    );
    assert_ne!(
        state,
        Some(AgentHookTaskState::WaitingApproval {
            tool_name: Some("Bash".to_string()),
        }),
        "PreToolUse 不应映射为 WaitingApproval"
    );
}

/// PermissionRequest 与 failure hook 应分别映射为 WaitingApproval 与 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   PermissionRequest raw.tool_name="Bash"
///   PostToolUseFailure raw.message="tool failed"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse PermissionRequest → task hint
///   2. parse PostToolUseFailure → task hint
///
/// 预期结果：
///   - 正断言：PermissionRequest -> WaitingApproval(Bash)
///   - 正断言：PostToolUseFailure -> Failed("tool failed")
///   - 负断言：failure 不应丢失 message
#[test]
fn test_permission_and_failure_events_map_to_expected_task_hints() {
    let permission = parse_hook_event(
        HookSource::Claude,
        11,
        json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"}),
    )
    .expect("PermissionRequest hook JSON 应可解析");
    let failure = parse_hook_event(
        HookSource::Claude,
        11,
        json!({"hook_event_name": "PostToolUseFailure", "message": "tool failed"}),
    )
    .expect("failure hook JSON 应可解析");

    let permission_state = task_state_for_provider_event(&permission);
    let failure_state = task_state_for_provider_event(&failure);

    assert_eq!(
        permission_state,
        Some(AgentHookTaskState::WaitingApproval {
            tool_name: Some("Bash".to_string()),
        }),
        "PermissionRequest 应映射为 WaitingApproval(Bash)"
    );
    assert_eq!(
        failure_state,
        Some(AgentHookTaskState::Failed {
            message: "tool failed".to_string(),
        }),
        "PostToolUseFailure 应映射为 Failed，并保留 message"
    );
    assert_ne!(
        failure_state,
        Some(AgentHookTaskState::Failed {
            message: String::new(),
        }),
        "failure message 不应被丢弃为空字符串"
    );
}

/// hook install 应追加 Multisoul command 且重复执行不重复添加。
///
/// 数据构造（含关键数值的推导过程）：
///   command_path="/tmp/multisoul-agent-hook"
///   target=All → Claude 与 Codex 都写入
///   install 执行 2 次，期望 command 出现次数仍等于 provider event 数
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建临时 Claude/Codex config path
///   2. install_hook_config(All)
///   3. 再次 install_hook_config(All)
///   4. 读取两个 JSON 文件并统计 command 字符串出现次数
///
/// 预期结果：
///   - 正断言：Claude/Codex config 都包含对应 forward command
///   - 正断言：重复 install 后 command 数量未翻倍
///   - 负断言：Codex config 不应写入 claude source command
#[test]
fn test_install_hook_config_is_idempotent_for_claude_and_codex() {
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let claude_path = temp.path().join("claude-settings.json");
    let codex_path = temp.path().join("codex-hooks.json");
    let paths = HookInstallPaths {
        claude_settings: claude_path.clone(),
        codex_hooks: codex_path.clone(),
    };
    let command_path = temp.path().join("multisoul-agent-hook");

    install_hook_config(HookTarget::All, &command_path, &paths).expect("第一次 install 应成功");
    install_hook_config(HookTarget::All, &command_path, &paths).expect("第二次 install 应成功");

    let claude_json = std::fs::read_to_string(&claude_path).expect("应能读取 Claude config");
    let codex_json = std::fs::read_to_string(&codex_path).expect("应能读取 Codex config");
    let claude_command = build_hook_command(&command_path, HookSource::Claude);
    let codex_command = build_hook_command(&command_path, HookSource::Codex);

    assert!(
        claude_json.contains(&claude_command),
        "Claude config 应包含 Multisoul Claude forward command"
    );
    assert!(
        codex_json.contains(&codex_command),
        "Codex config 应包含 Multisoul Codex forward command"
    );
    assert_eq!(
        claude_json.matches(&claude_command).count(),
        hook_events_for_source(HookSource::Claude).len(),
        "重复 install 后 Claude command 数量应等于 Claude event 数，不能翻倍"
    );
    assert_eq!(
        codex_json.matches(&codex_command).count(),
        hook_events_for_source(HookSource::Codex).len(),
        "重复 install 后 Codex command 数量应等于 Codex event 数，不能翻倍"
    );
    assert!(
        !codex_json.contains(&claude_command),
        "Codex config 不应写入 Claude source command"
    );
}

/// hook bridge 缺少 Multisoul env 时应 no-op 成功，避免影响 provider。
///
/// 数据构造（含关键数值的推导过程）：
///   env=None，表示缺少 MULTISOUL_NODE_ID / endpoint / token
///   raw.hook_event_name="UserPromptSubmit"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. forward_hook(Claude, raw, None)
///   2. 检查返回 outcome
///
/// 预期结果：
///   - 正断言：outcome=Noop
///   - 负断言：不应返回 FailedButIgnored，否则缺少 env 被误当成网络失败
#[test]
fn test_forward_hook_noops_when_multisoul_env_is_missing() {
    let raw = json!({"hook_event_name": "UserPromptSubmit"});

    let outcome = forward_hook(HookSource::Claude, raw, None);

    assert_eq!(
        outcome,
        HookForwardOutcome::Noop,
        "缺少 Multisoul env 时 hook bridge 应 no-op 成功"
    );
    assert_ne!(
        outcome,
        HookForwardOutcome::FailedButIgnored,
        "缺少 env 不是网络失败，不应返回 FailedButIgnored"
    );
}

/// hook bridge endpoint 不可达时应吞掉失败并成功返回。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id=7
///   endpoint=http://127.0.0.1:9/agent-hooks，discard 端口通常未监听
///   token="tok"
///   raw.hook_event_name="PreToolUse"，tool_name="Bash"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 HookForwardEnv
///   2. forward_hook(Codex, raw, Some(env))
///   3. POST 失败被转换为 FailedButIgnored
///
/// 预期结果：
///   - 正断言：outcome=FailedButIgnored
///   - 负断言：不应 panic 或返回 Posted
#[test]
fn test_forward_hook_ignores_unreachable_endpoint() {
    let raw = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"});
    let env = HookForwardEnv {
        node_id: 7,
        endpoint: "http://127.0.0.1:9/agent-hooks".to_string(),
        token: "tok".to_string(),
    };

    let outcome = forward_hook(HookSource::Codex, raw, Some(env));

    assert_eq!(
        outcome,
        HookForwardOutcome::FailedButIgnored,
        "endpoint 不可达时应吞掉失败并返回 FailedButIgnored"
    );
    assert_ne!(
        outcome,
        HookForwardOutcome::Posted,
        "endpoint 不可达时不应报告 Posted"
    );
}
