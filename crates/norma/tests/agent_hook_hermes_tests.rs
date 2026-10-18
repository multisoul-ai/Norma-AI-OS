use norma::agent_hook::{
    agent_hooks_installed, agent_hooks_installed_for_source, install_hook_config,
    observe_conversation, parse_hook_event, parse_hook_source, parse_hook_target,
    task_state_for_provider_event, uninstall_hook_config, AgentHookTaskState,
    ConversationObservationKind, HookInstallPaths, HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;
use std::path::Path;

fn paths_in(root: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: root.join("claude.json"),
        codex_hooks: root.join("codex.json"),
        cursor_hooks: root.join("cursor.json"),
        grok_hooks: root.join("grok.json"),
        opencode_plugin: root.join("opencode.js"),
        hermes_plugin_dir: root.join("hermes/plugins/norma-agent-hook"),
    }
}

/// Hermes 插件翻译后的事件应映射到五种状态，会话 ID 取自 session_id。
///
/// 数据构造（均为实测插件事件）：
///   SessionStart 无 session_id -> Ready；UserPromptSubmit -> Thinking；
///   PreToolUse tool_name=terminal -> RunningTool；PermissionRequest 带缓存工具名 -> WaitingApproval；
///   StopFailure message=auth -> Failed；Stop -> Done；SessionEnd -> Ended。
/// 预期：正断言各状态精确匹配；负断言 profile 不是 Grok，SessionStart 的 session_id 为 None。
#[test]
fn hermes_events_map_to_task_states() {
    let sid = "20261018_113636_195f7f";
    let state_of = |raw: serde_json::Value| {
        let event = parse_hook_event(HookSource::Hermes, 9, raw).expect("事件应可解析");
        assert_eq!(
            event.profile,
            AgentProfile::Hermes,
            "profile 必须是 Hermes 而非 Grok"
        );
        (task_state_for_provider_event(&event), event)
    };
    let (ready, ev) = state_of(json!({"hook_event_name": "SessionStart"}));
    assert_eq!(ready, Some(AgentHookTaskState::Ready), "启动应为 Ready");
    assert_eq!(
        ev.session_id, None,
        "register() 阶段的启动事件没有 session_id"
    );
    let (thinking, ev) =
        state_of(json!({"hook_event_name": "UserPromptSubmit", "session_id": sid}));
    assert_eq!(
        thinking,
        Some(AgentHookTaskState::Thinking),
        "提问应为 Thinking"
    );
    assert_eq!(
        ev.session_id.as_deref(),
        Some(sid),
        "session_id 应被保留供缓存"
    );
    let (running, _) = state_of(json!({"hook_event_name": "PreToolUse", "tool_name": "terminal"}));
    assert_eq!(
        running,
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("terminal".into())
        }),
        "工具调用应为 RunningTool"
    );
    let (waiting, _) =
        state_of(json!({"hook_event_name": "PermissionRequest", "tool_name": "terminal"}));
    assert_eq!(
        waiting,
        Some(AgentHookTaskState::WaitingApproval {
            tool_name: Some("terminal".into())
        }),
        "审批菜单应为 WaitingApproval"
    );
    let (failed, _) = state_of(json!({"hook_event_name": "StopFailure", "message": "auth"}));
    assert_eq!(
        failed,
        Some(AgentHookTaskState::Failed {
            message: "auth".into()
        }),
        "401 失败应为 Failed 且带原因"
    );
    assert_eq!(
        state_of(json!({"hook_event_name": "Stop"})).0,
        Some(AgentHookTaskState::Done),
        "回合结束应为 Done"
    );
    assert_eq!(
        state_of(json!({"hook_event_name": "SessionEnd"})).0,
        Some(AgentHookTaskState::Ended),
        "退出应为 Ended"
    );
}

/// 来源/目标字符串解析包含 hermes，未知值被拒绝。
#[test]
fn hermes_source_and_target_parse() {
    assert_eq!(
        parse_hook_source("hermes"),
        Some(HookSource::Hermes),
        "hermes 来源应可解析"
    );
    assert_eq!(
        parse_hook_target("Hermes"),
        Some(HookTarget::Hermes),
        "Hermes 目标应可解析"
    );
    assert_eq!(parse_hook_source("hermesx"), None, "未知来源必须拒绝");
}

/// 安装写入带标记的插件目录，卸载只删自有文件。
///
/// 数据构造：临时目录下的 hermes_plugin_dir；先安装再卸载。
/// 预期：正断言——两个文件存在、含标记与 MULTISOUL_HOOK_ENDPOINT、安装检测为真；
/// 负断言——插件源码不含 conversation_history/request_messages/user_message/tool_input
/// （隐私白名单）；只装 Hermes 时 All 不为真；卸载后目录消失且检测为假。
#[test]
fn hermes_install_writes_marked_plugin_and_uninstall_removes_it() {
    let temp = tempfile::tempdir().expect("临时目录");
    let paths = paths_in(temp.path());
    let command = temp.path().join("multisoul-agent-hook");
    assert!(
        !agent_hooks_installed_for_source(HookSource::Hermes, &paths),
        "安装前应为未安装"
    );
    install_hook_config(HookTarget::Hermes, &command, &paths).expect("安装应成功");
    let init =
        std::fs::read_to_string(paths.hermes_plugin_dir.join("__init__.py")).expect("插件源码");
    assert!(
        init.starts_with("# multisoul-hermes-plugin-marker"),
        "首行必须是标记"
    );
    assert!(
        init.contains("MULTISOUL_HOOK_ENDPOINT"),
        "插件应读取 Norma 端点环境变量"
    );
    assert!(
        init.contains("timeout=_TIMEOUT_SECONDS"),
        "POST 必须有超时，失败开放"
    );
    for forbidden in [
        "conversation_history",
        "request_messages",
        "user_message",
        "tool_input",
    ] {
        assert!(!init.contains(forbidden), "插件不得转发 {forbidden}");
    }
    assert!(
        paths.hermes_plugin_dir.join("plugin.yaml").is_file(),
        "应写入 plugin.yaml"
    );
    assert!(
        agent_hooks_installed_for_source(HookSource::Hermes, &paths),
        "安装后应检测到"
    );
    assert!(
        !agent_hooks_installed(HookTarget::All, &paths),
        "只装了 Hermes 时 All 不应为真"
    );
    uninstall_hook_config(HookTarget::Hermes, &paths).expect("卸载应成功");
    assert!(
        !paths.hermes_plugin_dir.exists(),
        "卸载后自有插件目录应消失"
    );
    assert!(
        !agent_hooks_installed_for_source(HookSource::Hermes, &paths),
        "卸载后应为未安装"
    );
}

/// 非自有插件目录必须拒绝覆盖，卸载也不得删除。
///
/// 数据构造：目录内有无标记的 __init__.py（内容 "user plugin"）。
/// 预期：安装返回错误且内容不变；卸载成功但文件仍在。
#[test]
fn hermes_refuses_to_overwrite_or_remove_unmanaged_plugin() {
    let temp = tempfile::tempdir().expect("临时目录");
    let paths = paths_in(temp.path());
    std::fs::create_dir_all(&paths.hermes_plugin_dir).expect("建目录");
    let init = paths.hermes_plugin_dir.join("__init__.py");
    std::fs::write(&init, "user plugin").expect("写用户插件");
    let command = temp.path().join("multisoul-agent-hook");
    let result = install_hook_config(HookTarget::Hermes, &command, &paths);
    assert!(result.is_err(), "非自有目录必须拒绝覆盖");
    assert_eq!(
        std::fs::read_to_string(&init).unwrap(),
        "user plugin",
        "用户文件内容不得改变"
    );
    assert!(
        !agent_hooks_installed_for_source(HookSource::Hermes, &paths),
        "非自有不算已安装"
    );
    uninstall_hook_config(HookTarget::Hermes, &paths).expect("卸载非自有应安全跳过");
    assert!(init.is_file(), "卸载不得删除非自有文件");
}

/// Stop 事件携带 last_assistant_message 时产生完整回复观察，无正文则 unavailable。
///
/// 数据构造：正文 "done."；另一条正文 1201 个 'x'（比上限 1200 多 1）；第三条无正文。
/// 预期：第一条 CompleteAgentReply 且文本保留；第二条 truncated=true；第三条 unavailable。
#[test]
fn hermes_stop_reply_feeds_conversation_observation() {
    let with_reply = parse_hook_event(
        HookSource::Hermes,
        3,
        json!({"hook_event_name": "Stop", "session_id": "s1", "last_assistant_message": "done."}),
    )
    .unwrap();
    let observed = observe_conversation(&with_reply).expect("应有观察");
    assert_eq!(
        observed.kind,
        ConversationObservationKind::CompleteAgentReply,
        "有正文应为完整回复"
    );
    assert_eq!(observed.text.as_deref(), Some("done."), "正文应原样保留");
    let long = "x".repeat(1201);
    let truncated = parse_hook_event(
        HookSource::Hermes,
        3,
        json!({"hook_event_name": "Stop", "last_assistant_message": long}),
    )
    .unwrap();
    assert!(
        observe_conversation(&truncated).unwrap().truncated,
        "超长回复必须标记截断"
    );
    let empty =
        parse_hook_event(HookSource::Hermes, 3, json!({"hook_event_name": "Stop"})).unwrap();
    assert_eq!(
        observe_conversation(&empty).unwrap().kind,
        ConversationObservationKind::UnavailableCompleteAgentReply,
        "无正文不得伪造回复"
    );
}
