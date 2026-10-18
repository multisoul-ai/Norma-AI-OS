use norma::agent_hook::{
    agent_hooks_installed_for_source, install_hook_config, parse_hook_event,
    task_state_for_provider_event, uninstall_hook_config, AgentHookTaskState, HookInstallPaths,
    HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;

/// OpenCode hook JSON should normalize to the OpenCode Agent profile and existing task states.
///
/// Data setup:
///   source=OpenCode -> profile=AgentProfile::OpenCode
///   node_id=15
///   raw.hook_event_name="PreToolUse"
///   raw.tool_name="Edit"
///   raw.session_id="opencode-session-1"
///
/// Execution:
///   1. parse_hook_event(OpenCode, 15, raw)
///   2. task_state_for_provider_event(event)
///
/// Expected:
///   - Positive: profile is OpenCode and session_id/tool_name are preserved.
///   - Positive: PreToolUse maps to RunningTool(Some("Edit")).
///   - Negative: OpenCode must not be mislabeled as Codex.
#[test]
fn test_opencode_pre_tool_use_maps_to_running_tool() {
    let raw = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "session_id": "opencode-session-1"
    });

    let event = parse_hook_event(HookSource::OpenCode, 15, raw).expect("hook JSON 应可解析");
    let state = task_state_for_provider_event(&event);

    assert_eq!(
        event.profile,
        AgentProfile::OpenCode,
        "source=OpenCode 应归一化 profile=OpenCode"
    );
    assert_eq!(
        event.session_id,
        Some("opencode-session-1".to_string()),
        "OpenCode hook 应保留 session_id"
    );
    assert_eq!(
        state,
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("Edit".to_string())
        }),
        "OpenCode PreToolUse 应映射为 RunningTool(Edit)"
    );
    assert_ne!(
        event.profile,
        AgentProfile::Codex,
        "OpenCode hook 不应被误标为 Codex"
    );
}

/// OpenCode install writes a Norma-owned plugin file and uninstall removes only that plugin.
///
/// Data setup:
///   command_path=/tmp/multisoul-agent-hook
///   opencode_plugin=<temp>/multisoul-agent-hook.js
///   target=OpenCode, represented by one standalone JS plugin file.
///
/// Execution:
///   1. install_hook_config(OpenCode)
///   2. agent_hooks_installed_for_source(OpenCode) should become true.
///   3. Read plugin source and verify it forwards OpenCode events to Norma.
///   4. uninstall_hook_config(OpenCode)
///
/// Expected:
///   - Positive: plugin file exists after install and contains the Norma marker.
///   - Positive: installed detection toggles true then false.
///   - Negative: plugin file should not remain after uninstall.
#[test]
fn test_opencode_install_writes_and_removes_plugin_file() {
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let plugin_path = temp.path().join("multisoul-agent-hook.js");
    let paths = HookInstallPaths {
        claude_settings: temp.path().join("claude-settings.json"),
        codex_hooks: temp.path().join("codex-hooks.json"),
        cursor_hooks: temp.path().join("cursor-hooks.json"),
        grok_hooks: temp.path().join("grok-hooks.json"),
        pi_extension: temp.path().join("norma-agent-hook.ts"),
        opencode_plugin: plugin_path.clone(),
        kimi_config: (plugin_path.clone()).with_file_name("kimi-config.toml"),
    };
    let command_path = temp.path().join("multisoul-agent-hook");

    assert!(
        !agent_hooks_installed_for_source(HookSource::OpenCode, &paths),
        "install 前 OpenCode 不应报告 Multisoul plugin 已安装"
    );

    install_hook_config(HookTarget::OpenCode, &command_path, &paths).expect("install 应成功");
    assert!(
        plugin_path.is_file(),
        "OpenCode install 应创建 Norma-owned plugin 文件"
    );
    assert!(
        agent_hooks_installed_for_source(HookSource::OpenCode, &paths),
        "install 后 OpenCode 应报告 Multisoul plugin 已安装"
    );

    let plugin = std::fs::read_to_string(&plugin_path).expect("应能读取 OpenCode plugin");
    assert!(
        plugin.contains("multisoul-opencode-plugin-marker"),
        "OpenCode plugin 应包含 Norma marker 以便安全检测和卸载"
    );
    assert!(
        plugin.contains("MULTISOUL_HOOK_ENDPOINT"),
        "OpenCode plugin 应读取 Norma endpoint env 并转发事件"
    );
    assert!(
        !plugin.contains("codex-hooks.json"),
        "OpenCode plugin 不应写入或依赖 Codex hooks.json"
    );

    uninstall_hook_config(HookTarget::OpenCode, &paths).expect("uninstall 应成功");
    assert!(
        !agent_hooks_installed_for_source(HookSource::OpenCode, &paths),
        "uninstall 后 OpenCode 不应继续报告 Multisoul plugin 已安装"
    );
    assert!(
        !plugin_path.exists(),
        "uninstall 后 Norma-owned OpenCode plugin 文件应被删除"
    );
}

/// OpenCode 用户插件冲突：同路径存在非 Norma 文件时安装不得覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   target path files  = 1
///   Norma marker count = 0
///   user source bytes  = "export const userPlugin = true;\n" = 32 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Norma 目标插件路径预置无 marker 的用户文件。
///   2. 调用生产 install_hook_config(OpenCode)。
///   3. 重新读取同一路径。
///
/// 预期结果：
///   - 正断言：安装返回冲突错误，用户内容逐字保留。
///   - 负断言：不得静默写入 Norma marker 或覆盖用户插件。
#[test]
fn test_opencode_install_rejects_unmanaged_same_path_plugin() {
    let temp = tempfile::tempdir().expect("OpenCode 冲突测试必须创建临时目录");
    let plugin_path = temp.path().join("multisoul-agent-hook.js");
    let paths = HookInstallPaths {
        claude_settings: temp.path().join("claude-settings.json"),
        codex_hooks: temp.path().join("codex-hooks.json"),
        cursor_hooks: temp.path().join("cursor-hooks.json"),
        grok_hooks: temp.path().join("grok-hooks.json"),
        pi_extension: temp.path().join("norma-agent-hook.ts"),
        opencode_plugin: plugin_path.clone(),
        kimi_config: (plugin_path.clone()).with_file_name("kimi-config.toml"),
    };
    let user_source = "export const userPlugin = true;\n";
    std::fs::write(&plugin_path, user_source).expect("测试必须预置用户 OpenCode 插件");

    let result = install_hook_config(
        HookTarget::OpenCode,
        &temp.path().join("multisoul-agent-hook"),
        &paths,
    );
    let preserved =
        std::fs::read_to_string(&plugin_path).expect("冲突返回后用户 OpenCode 插件必须仍可读取");

    assert!(
        result.is_err(),
        "同路径存在无 Norma marker 的插件时必须返回冲突错误"
    );
    assert_eq!(
        preserved, user_source,
        "OpenCode 安装不得覆盖无法确认归属的用户插件"
    );
    assert!(
        !preserved.contains(norma::agent_hook::OPENCODE_PLUGIN_MARKER),
        "冲突文件不得被静默替换成 Norma 受管插件"
    );
}
