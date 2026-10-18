use norma::agent_hook::{
    build_hook_command, hook_events_for_source, install_hook_config, parse_hook_event,
    task_state_for_provider_event, uninstall_hook_config, AgentHookTaskState, HookInstallPaths,
    HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;

/// Cursor CLI 的小写 lifecycle payload 应转换成既有的标准任务状态。
///
/// 数据构造（含关键数值的推导过程）：
///   source=Cursor → profile=AgentProfile::Cursor；node_id=12 来自 MULTISOUL_NODE_ID。
///   conversation_id="cursor-session-12" 是 Cursor 的可恢复会话标识。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `parse_hook_event(Cursor, 12, raw)` 读取 Cursor 原始字段。
///   2. `task_state_for_provider_event(event)` 归一化为 Thinking。
///   3. 读取 profile、provider_event 与 session_id。
///
/// 预期结果：
///   - 断言 A：Cursor provider 映射为 Cursor profile，prompt 事件进入 Thinking。
///   - 断言 B：conversation_id 被缓存为 session_id，供恢复使用。
///   - 断言 C：事件不应误映射为 Done 或 Codex profile。
#[test]
fn test_cursor_before_submit_prompt_maps_to_thinking_and_caches_conversation() {
    let raw = json!({
        "hook_event_name": "beforeSubmitPrompt",
        "conversation_id": "cursor-session-12",
        "prompt": "review the hook bridge"
    });

    let event = parse_hook_event(HookSource::Cursor, 12, raw).expect("Cursor hook JSON 应可解析");
    let state = task_state_for_provider_event(&event);

    assert_eq!(
        event.profile,
        AgentProfile::Cursor,
        "Cursor source 应归一化为 Cursor profile"
    );
    assert_eq!(
        event.provider_event, "UserPromptSubmit",
        "Cursor beforeSubmitPrompt 应归一化为标准 prompt 事件"
    );
    assert_eq!(
        state,
        Some(AgentHookTaskState::Thinking),
        "Cursor prompt 提交应让卡片进入 Thinking"
    );
    assert_eq!(
        event.session_id,
        Some("cursor-session-12".to_string()),
        "Cursor conversation_id 应保存为可恢复 session_id"
    );
    assert_ne!(
        state,
        Some(AgentHookTaskState::Done),
        "提交 prompt 不应直接标记任务完成"
    );
    assert_ne!(
        event.profile,
        AgentProfile::Codex,
        "Cursor hook 不应误归属为 Codex"
    );
}

/// Cursor Hook 的 generation_id 是单轮稳定身份，必须与跨多轮稳定的 conversation_id 分开保存。
///
/// 数据构造：同一 conversation_id="cursor-session-12"，当前 generation_id="cursor-turn-3"，并携带官方 transcript_path。
/// 执行过程：解析 beforeSubmitPrompt → 读取 session_id、turn_id 与原始 transcript_path。
/// 预期结果：session 使用 conversation_id，turn 使用 generation_id；二者不得相等，原路径不得丢失或被客户端字段替换。
#[test]
fn cursor_generation_and_transcript_fields_preserve_exact_mobile_identity() {
    let raw = json!({
        "hook_event_name": "beforeSubmitPrompt",
        "conversation_id": "cursor-session-12",
        "generation_id": "cursor-turn-3",
        "transcript_path": "/tmp/cursor-session-12.jsonl",
        "prompt": "检查 Cursor 卡片"
    });

    let event =
        parse_hook_event(HookSource::Cursor, 12, raw).expect("Cursor 官方 Hook 字段应可解析");

    assert_eq!(
        event.session_id.as_deref(),
        Some("cursor-session-12"),
        "conversation_id 必须作为跨轮会话身份"
    );
    assert_eq!(
        event.turn_id.as_deref(),
        Some("cursor-turn-3"),
        "generation_id 必须作为本轮回执身份"
    );
    assert_ne!(
        event.session_id, event.turn_id,
        "会话身份不能被错误复用为每轮身份"
    );
    assert_eq!(
        event.raw_json["transcript_path"], "/tmp/cursor-session-12.jsonl",
        "Cursor 提供的本地 transcript_path 必须保留给受控后台读取"
    );
}

/// Cursor 同时携带通用字段与官方字段时，官方身份必须拥有更高优先级。
///
/// 数据构造：通用 session_id/turn_id 指向 stale 身份，Cursor 官方 conversation_id/generation_id 指向当前身份。
/// 执行过程：解析同一条 beforeSubmitPrompt Hook，读取归一化后的会话与回合身份。
/// 预期结果：只采用 Cursor 官方字段；不得让兼容字段覆盖当前会话或回合。
#[test]
fn cursor_official_identity_fields_override_generic_compatibility_fields() {
    let raw = json!({
        "hook_event_name": "beforeSubmitPrompt",
        "session_id": "stale-session",
        "turn_id": "stale-turn",
        "conversation_id": "cursor-session-current",
        "generation_id": "cursor-turn-current"
    });

    let event = parse_hook_event(HookSource::Cursor, 12, raw).expect("Cursor 冲突身份载荷应可解析");

    assert_eq!(
        event.session_id.as_deref(),
        Some("cursor-session-current"),
        "Cursor conversation_id 必须覆盖通用 session_id"
    );
    assert_eq!(
        event.turn_id.as_deref(),
        Some("cursor-turn-current"),
        "Cursor generation_id 必须覆盖通用 turn_id"
    );
    assert_ne!(
        event.session_id.as_deref(),
        Some("stale-session"),
        "兼容字段不得污染 Cursor 会话身份"
    );
    assert_ne!(
        event.turn_id.as_deref(),
        Some("stale-turn"),
        "兼容字段不得污染 Cursor 回合身份"
    );
}

/// Cursor Hook 安装与清理应只修改 `~/.cursor/hooks.json` 的 Norma 命令。
///
/// 数据构造（含关键数值的推导过程）：
///   Cursor 追踪 7 个 lifecycle 事件；重复安装次数 = 2；existing stop hook="echo keep-me"。
///   安装后命令数 = 事件数 = 7，卸载后该命令数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入带用户 stop hook 的临时 Cursor hooks.json。
///   2. 对 Cursor 安装两次，读取结果验证每个事件只有 1 个 Norma 命令。
///   3. 卸载 Cursor，读取结果验证只移除 Norma 命令。
///
/// 预期结果：
///   - 断言 A：配置包含 version=1 和每个 Cursor 事件的 bridge 命令。
///   - 断言 B：用户 stop hook 在安装和卸载后仍存在。
///   - 断言 C：配置不写入 Codex bridge，卸载后不残留 Cursor bridge。
#[test]
fn test_cursor_hook_config_is_idempotent_and_preserves_user_hooks() {
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let cursor_path = temp.path().join("cursor-hooks.json");
    std::fs::write(
        &cursor_path,
        r#"{"version":1,"hooks":{"stop":[{"command":"echo keep-me"}]}}"#,
    )
    .expect("应能写入已有 Cursor hook 配置");
    let paths = HookInstallPaths {
        claude_settings: temp.path().join("claude-settings.json"),
        codex_hooks: temp.path().join("codex-hooks.json"),
        cursor_hooks: cursor_path.clone(),
        grok_hooks: temp.path().join("grok-hooks.json"),
        pi_extension: temp.path().join("norma-agent-hook.ts"),
        opencode_plugin: temp.path().join("multisoul-agent-hook.js"),
        kimi_config: (temp.path().join("multisoul-agent-hook.js"))
            .with_file_name("kimi-config.toml"),
        hermes_plugin_dir: std::path::PathBuf::from("/nonexistent-hermes-plugin-dir"),
    };
    let command_path = temp.path().join("multisoul-agent-hook");
    let cursor_command = build_hook_command(&command_path, HookSource::Cursor);
    let codex_command = build_hook_command(&command_path, HookSource::Codex);

    install_hook_config(HookTarget::Cursor, &command_path, &paths)
        .expect("第一次 Cursor install 应成功");
    install_hook_config(HookTarget::Cursor, &command_path, &paths)
        .expect("第二次 Cursor install 应成功");

    let installed = std::fs::read_to_string(&cursor_path).expect("应能读取 Cursor config");
    assert!(
        installed.contains("\"version\": 1"),
        "Cursor hooks.json 应写入 version=1"
    );
    assert_eq!(
        installed.matches(&cursor_command).count(),
        hook_events_for_source(HookSource::Cursor).len(),
        "重复安装后 Cursor bridge 数量应等于追踪事件数，不能翻倍"
    );
    assert!(
        installed.contains("echo keep-me"),
        "安装 Cursor bridge 不应删除用户已有 stop hook"
    );
    assert!(
        !installed.contains(&codex_command),
        "Cursor config 不应写入 Codex bridge 命令"
    );

    uninstall_hook_config(HookTarget::Cursor, &paths).expect("Cursor uninstall 应成功");
    let uninstalled = std::fs::read_to_string(&cursor_path).expect("卸载后应能读取 Cursor config");
    assert!(
        uninstalled.contains("echo keep-me"),
        "卸载 Cursor bridge 后用户 stop hook 必须保留"
    );
    assert!(
        !uninstalled.contains(&cursor_command),
        "卸载后 Cursor bridge 命令不得残留"
    );
}
