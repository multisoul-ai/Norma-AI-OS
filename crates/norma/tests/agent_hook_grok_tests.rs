use norma::agent_hook::{
    agent_hooks_installed_for_source, build_hook_command, hook_events_for_source,
    install_hook_config, parse_hook_event, task_state_for_provider_event, uninstall_hook_config,
    AgentHookTaskState, HookInstallPaths, HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;

/// Grok Build PreToolUse：官方 camelCase Hook 载荷应归一化为 Norma 的运行工具状态。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 23（与 session 后缀一致，便于识别路由目标）
///   hookEventName = "PreToolUse"（Grok 官方字段名）
///   sessionId = "grok-session-23"（Grok 官方 camelCase 会话字段）
///   toolName = "Bash"（本次工具调用名称）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. parse_hook_event(Grok, 23, raw) 读取官方 camelCase 字段；
///   2. 将 source 映射为 AgentProfile::Grok；
///   3. 将 PreToolUse 映射为 RunningTool，并保留工具名。
///
/// 预期结果：
///   - 正断言：profile、session_id、tool_name 与 RunningTool(Bash) 均正确；
///   - 负断言：profile 不得误标为 Codex，状态不得误标为 WaitingApproval。
#[test]
fn grok_camel_case_pre_tool_use_maps_to_running_tool() {
    let event = parse_hook_event(
        HookSource::Grok,
        23,
        json!({
            "hookEventName": "PreToolUse",
            "sessionId": "grok-session-23",
            "toolName": "Bash",
            "toolInput": {"command": "cargo test -p norma"}
        }),
    )
    .expect("Grok 官方 camelCase PreToolUse 应可解析");

    assert_eq!(
        event.profile,
        AgentProfile::Grok,
        "Grok hook 必须保留 Grok profile"
    );
    assert_ne!(
        event.profile,
        AgentProfile::Codex,
        "Grok hook 不得被误标为 Codex profile"
    );
    assert_eq!(
        event.session_id.as_deref(),
        Some("grok-session-23"),
        "Grok sessionId 应写入标准 session_id"
    );
    assert_eq!(
        event.tool_name.as_deref(),
        Some("Bash"),
        "Grok toolName 应写入标准 tool_name"
    );
    assert_eq!(
        task_state_for_provider_event(&event),
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("Bash".to_string()),
        }),
        "Grok PreToolUse 应映射为 RunningTool(Bash)"
    );
    assert_ne!(
        task_state_for_provider_event(&event),
        Some(AgentHookTaskState::WaitingApproval {
            tool_name: Some("Bash".to_string()),
        }),
        "Grok PreToolUse 不得误映射为 WaitingApproval"
    );
}

/// Grok Build Hook 安装：写入专属配置、幂等安装，并在卸载时保留第三方 Hook。
///
/// 数据构造（含关键数值的推导过程）：
///   target = Grok；官方事件数 = hook_events_for_source(Grok).len()
///   install 次数 = 2 → 幂等后 Norma command 数仍应等于官方事件数
///   第三方 PreToolUse command 数 = 1 → install/uninstall 前后都必须保持 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时 Grok Hook JSON 中预置一个第三方 PreToolUse command；
///   2. 连续两次 install_hook_config(Grok)，写入 `forward --source grok`；
///   3. 验证专属文件已被识别为安装完成，且没有重复 command；
///   4. uninstall_hook_config(Grok)，只移除 Norma command；
///   5. 重新读取 JSON，确认第三方 Hook 仍存在。
///
/// 预期结果：
///   - 正断言：安装后每个 Grok 事件仅有一个 Norma command，第三方 command 保留；
///   - 正断言：卸载后第三方 command 仍保留；
///   - 负断言：卸载后不得残留 `forward --source grok`，也不得写入 Codex source。
#[test]
fn grok_hook_install_is_idempotent_and_uninstall_preserves_third_party_hooks() {
    let temp = tempfile::tempdir().expect("应能创建 Grok Hook 临时目录");
    let grok_path = temp.path().join("grok-hooks.json");
    let third_party_command = "third-party-observer --event pre-tool";
    std::fs::write(
        &grok_path,
        serde_json::to_string_pretty(&json!({
            "hooks": {
                "PreToolUse": [{
                    "hooks": [{
                        "type": "command",
                        "command": third_party_command,
                        "timeout": 5
                    }]
                }]
            }
        }))
        .expect("第三方 Grok Hook JSON 应可序列化"),
    )
    .expect("应能写入第三方 Grok Hook 配置");
    let paths = HookInstallPaths {
        claude_settings: temp.path().join("claude-settings.json"),
        codex_hooks: temp.path().join("codex-hooks.json"),
        cursor_hooks: temp.path().join("cursor-hooks.json"),
        grok_hooks: grok_path.clone(),
        pi_extension: temp.path().join("norma-agent-hook.ts"),
        opencode_plugin: temp.path().join("multisoul-agent-hook.js"),
        kimi_config: (temp.path().join("multisoul-agent-hook.js"))
            .with_file_name("kimi-config.toml"),
    };
    let command_path = temp.path().join("multisoul-agent-hook");
    let grok_command = build_hook_command(&command_path, HookSource::Grok);
    let codex_command = build_hook_command(&command_path, HookSource::Codex);

    install_hook_config(HookTarget::Grok, &command_path, &paths)
        .expect("第一次 Grok Hook install 应成功");
    install_hook_config(HookTarget::Grok, &command_path, &paths)
        .expect("第二次 Grok Hook install 应保持幂等");

    let installed = std::fs::read_to_string(&grok_path).expect("应能读取已安装的 Grok 配置");
    assert!(
        agent_hooks_installed_for_source(HookSource::Grok, &paths),
        "专属 Grok Hook 文件应被识别为已安装"
    );
    assert_eq!(
        installed.matches(&grok_command).count(),
        hook_events_for_source(HookSource::Grok).len(),
        "重复安装后 Grok command 数应等于官方事件数，不能翻倍"
    );
    assert_eq!(
        installed.matches(third_party_command).count(),
        1,
        "安装 Grok Hook 后必须保留且只保留一份第三方 command"
    );
    assert!(
        !installed.contains(&codex_command),
        "Grok Hook 配置不得写入 Codex source command"
    );

    uninstall_hook_config(HookTarget::Grok, &paths).expect("Grok Hook uninstall 应成功");
    let uninstalled = std::fs::read_to_string(&grok_path).expect("应能读取卸载后的 Grok 配置");
    assert_eq!(
        uninstalled.matches(third_party_command).count(),
        1,
        "卸载 Grok Hook 后必须保留第三方 command"
    );
    assert!(
        !uninstalled.contains(&grok_command),
        "卸载 Grok Hook 后不得残留 Norma forward command"
    );
    assert!(
        !agent_hooks_installed_for_source(HookSource::Grok, &paths),
        "移除全部 Norma Grok command 后不应仍报告已安装"
    );
}
