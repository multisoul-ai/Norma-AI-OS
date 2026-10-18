use norma::agent_hook::{
    agent_hooks_installed, agent_hooks_installed_for_source, build_hook_command,
    hook_events_for_source, install_hook_config, kimi_config_path_for, observe_conversation,
    parse_hook_event, parse_hook_source, parse_hook_target, task_state_for_provider_event,
    uninstall_hook_config, AgentHookTaskState, ConversationObservationKind, HookInstallPaths,
    HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn paths(root: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: root.join("claude-settings.json"),
        codex_hooks: root.join("codex-hooks.json"),
        cursor_hooks: root.join("cursor-hooks.json"),
        grok_hooks: root.join("grok-hooks.json"),
        opencode_plugin: root.join("multisoul-agent-hook.js"),
        kimi_config: root.join("kimi-home").join("config.toml"),
    }
}

fn state(raw: serde_json::Value) -> Option<AgentHookTaskState> {
    let event = parse_hook_event(HookSource::Kimi, 7, raw).expect("Kimi 载荷应可解析");
    task_state_for_provider_event(&event)
}

/// Kimi 事件映射：用 2.2.0 真机载荷字段验证五状态映射，包括 Kimi 专有的两个事件。
///
/// 数据构造（含关键数值的推导过程）：
///   载荷字段取自 docs/research/evidence/kimi/hook-payload-samples.jsonl；
///   StopFailure 的 error_message = "400 mock failure"；Interrupt 的 reason = "cancelled"。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐个事件调用 parse_hook_event(Kimi, 7, raw) 与 task_state_for_provider_event；
///   2. 检查 profile、session_id、tool_name、message。
///
/// 预期结果：
///   - 正断言：PermissionRequest=WaitingApproval(Bash)，PermissionResult=Thinking，
///     Interrupt=Done，StopFailure=Failed("400 mock failure")，Stop=Done，SessionEnd=Ended；
///   - 负断言：PermissionResult 不得被当成等待审批；profile 不得为 Grok。
#[test]
fn kimi_events_map_to_five_states() {
    let sid = "session_e9029623-7275-4b57-8758-0e8cb054234f";
    let event = parse_hook_event(
        HookSource::Kimi,
        7,
        json!({"hook_event_name": "PreToolUse", "session_id": sid, "tool_name": "Bash"}),
    )
    .expect("PreToolUse 应可解析");
    assert_eq!(event.profile, AgentProfile::Kimi, "profile 必须是 Kimi");
    assert_ne!(event.profile, AgentProfile::Grok, "不得误标为 Grok");
    assert_eq!(
        event.session_id.as_deref(),
        Some(sid),
        "session_id 应取自载荷"
    );
    assert_eq!(
        task_state_for_provider_event(&event),
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("Bash".to_string())
        }),
        "PreToolUse 应映射为 RunningTool"
    );
    assert_eq!(
        state(json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"})),
        Some(AgentHookTaskState::WaitingApproval {
            tool_name: Some("Bash".to_string())
        }),
        "PermissionRequest 是等待审批信号"
    );
    let result = state(json!({"hook_event_name": "PermissionResult", "decision": "rejected"}));
    assert_eq!(
        result,
        Some(AgentHookTaskState::Thinking),
        "PermissionResult 应把状态拉回 Thinking"
    );
    assert_ne!(
        result,
        Some(AgentHookTaskState::WaitingApproval { tool_name: None }),
        "PermissionResult 不得停留在等待审批"
    );
    assert_eq!(
        state(json!({"hook_event_name": "Interrupt", "reason": "cancelled"})),
        Some(AgentHookTaskState::Done),
        "Interrupt 应映射为 Done"
    );
    assert_eq!(
        state(json!({
            "hook_event_name": "StopFailure",
            "error_type": "APIStatusError",
            "error_message": "400 mock failure"
        })),
        Some(AgentHookTaskState::Failed {
            message: "400 mock failure".to_string()
        }),
        "StopFailure 的信息应取 error_message"
    );
    assert_eq!(
        state(json!({"hook_event_name": "Stop"})),
        Some(AgentHookTaskState::Done),
        "Stop 应映射为 Done"
    );
    assert_eq!(
        state(json!({"hook_event_name": "SessionEnd", "reason": "exit"})),
        Some(AgentHookTaskState::Ended),
        "SessionEnd 应映射为 Ended"
    );
    assert_eq!(
        state(json!({"hook_event_name": "SessionStart", "source": "startup"})),
        Some(AgentHookTaskState::Ready),
        "SessionStart 应映射为 Ready"
    );
    assert_eq!(
        state(json!({"hook_event_name": "UserPromptSubmit"})),
        Some(AgentHookTaskState::Thinking),
        "UserPromptSubmit 应映射为 Thinking"
    );
}

/// Interrupt 与 PermissionResult 只对 Kimi 生效：其他来源的同名事件保持原行为（无状态）。
///
/// 数据构造：Codex 来源载荷 hook_event_name = "Interrupt"。
/// 执行过程：解析后取状态。
/// 预期结果：负断言，Codex 的 Interrupt 不得产生状态；正断言，Kimi 的同载荷为 Done。
#[test]
fn kimi_only_events_do_not_leak_to_other_sources() {
    let raw = json!({"hook_event_name": "Interrupt", "reason": "cancelled"});
    let codex = parse_hook_event(HookSource::Codex, 7, raw.clone()).expect("应可解析");
    assert_eq!(
        task_state_for_provider_event(&codex),
        None,
        "Codex 的 Interrupt 不得被映射"
    );
    assert_eq!(
        state(raw),
        Some(AgentHookTaskState::Done),
        "同载荷来自 Kimi 时应为 Done"
    );
}

/// Kimi 提示词是 `[{type,text}]` 数组：对话证据必须拼出 text，且 Stop 无回复文本时降级为不可用。
///
/// 数据构造：prompt = [{"type":"text","text":"say"},{"type":"image"},{"type":"text","text":"hi"}]，
///   拼接规则为只取 type=text 的 text 并以换行连接，期望 "say\nhi"（2 段）。
/// 执行过程：解析 UserPromptSubmit 与 Stop，调用 observe_conversation。
/// 预期结果：用户消息文本 = "say\nhi"；Stop 为 UnavailableCompleteAgentReply 且 text 为 None；
///   负断言：用户消息不得为空，也不得包含 image。
#[test]
fn kimi_prompt_array_feeds_conversation_evidence() {
    let prompt = parse_hook_event(
        HookSource::Kimi,
        7,
        json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "session_a",
            "prompt": [
                {"type": "text", "text": "say"},
                {"type": "image"},
                {"type": "text", "text": "hi"}
            ]
        }),
    )
    .expect("应可解析");
    let observation = observe_conversation(&prompt).expect("应产生用户消息");
    assert_eq!(observation.kind, ConversationObservationKind::UserMessage);
    assert_eq!(
        observation.text.as_deref(),
        Some("say\nhi"),
        "应拼接 text 段"
    );
    assert!(
        !observation.text.as_deref().unwrap_or("").contains("image"),
        "非 text 段不得进入证据"
    );
    assert_eq!(
        norma::agent_hook::extract_prompt_text(&prompt.raw_json).as_deref(),
        Some("say\nhi"),
        "状态灯摘要同样需要读数组"
    );

    let stop = parse_hook_event(
        HookSource::Kimi,
        7,
        json!({"hook_event_name": "Stop", "session_id": "session_a", "stop_hook_active": false}),
    )
    .expect("应可解析");
    let reply = observe_conversation(&stop).expect("Stop 应产生观察");
    assert_eq!(
        reply.kind,
        ConversationObservationKind::UnavailableCompleteAgentReply,
        "Stop 载荷不含回复文本，必须降级"
    );
    assert_eq!(reply.text, None, "降级时不得编造回复文本");
}

/// 安装/幂等/卸载：只管理带标记的区块，保留用户内容与既有 [[hooks]]。
///
/// 数据构造：config.toml 初始 = 注释 + `[tui]` 表 + 用户自己的一条 [[hooks]]（共 5 行）；
///   Kimi 订阅 11 个事件，故区块内应有 11 条 `[[hooks]]`，加用户 1 条共 12 条。
/// 执行过程：安装两次（幂等）→ 检查 → 卸载。
/// 预期结果：两次安装后 `[[hooks]]` 总数 12、begin 标记 1 次、已安装检测为真；
///   卸载后文件与初始内容逐字节相同，检测为假；目标文件不存在时安装会创建目录与文件。
#[test]
fn kimi_install_is_idempotent_and_uninstall_keeps_user_content() {
    let temp = tempfile::tempdir().expect("临时目录");
    let paths = paths(temp.path());
    fs::create_dir_all(paths.kimi_config.parent().unwrap()).unwrap();
    let original = "# my kimi config\n[tui]\ntheme = \"dark\"\n\n[[hooks]]\nevent = \"Stop\"\ncommand = \"echo mine\"\n";
    fs::write(&paths.kimi_config, original).unwrap();
    let command_path = temp.path().join("bin dir").join("multisoul-agent-hook");

    install_hook_config(HookTarget::Kimi, &command_path, &paths).expect("首次安装");
    install_hook_config(HookTarget::Kimi, &command_path, &paths).expect("重复安装");
    let text = fs::read_to_string(&paths.kimi_config).unwrap();
    assert_eq!(
        hook_events_for_source(HookSource::Kimi).len(),
        11,
        "订阅 11 个事件"
    );
    assert_eq!(
        text.matches("[[hooks]]").count(),
        12,
        "11 条自有 + 1 条用户"
    );
    assert_eq!(
        text.matches(">>> multisoul-agent-hook managed").count(),
        1,
        "区块只能有一个"
    );
    assert!(text.starts_with(original), "用户原内容必须原样保留在前面");
    assert!(
        text.contains(&build_hook_command(&command_path, HookSource::Kimi)),
        "命令应是带引用路径的 forward --source kimi"
    );
    assert!(text.contains("timeout = 10"), "超时应为已验证的 10 秒");
    assert!(
        agent_hooks_installed_for_source(HookSource::Kimi, &paths),
        "应检测为已安装"
    );
    assert!(
        agent_hooks_installed(HookTarget::Kimi, &paths),
        "HookTarget 检测一致"
    );

    uninstall_hook_config(HookTarget::Kimi, &paths).expect("卸载");
    assert_eq!(
        fs::read_to_string(&paths.kimi_config).unwrap(),
        original,
        "卸载后必须逐字节恢复用户内容"
    );
    assert!(
        !agent_hooks_installed_for_source(HookSource::Kimi, &paths),
        "卸载后不应再检测为已安装"
    );

    let fresh = temp.path().join("fresh").join("config.toml");
    let mut fresh_paths = paths.clone();
    fresh_paths.kimi_config = fresh.clone();
    install_hook_config(HookTarget::Kimi, &command_path, &fresh_paths).expect("新建安装");
    assert!(fresh.is_file(), "配置不存在时应创建");
    uninstall_hook_config(HookTarget::Kimi, &fresh_paths).expect("卸载");
    assert_eq!(
        fs::read_to_string(&fresh).unwrap(),
        "",
        "只含自有区块的文件卸载后应为空"
    );
}

/// 拒绝覆盖：用户已有行内 `hooks = [...]` 或 `[hooks]` 表，或标记区块被破坏时，必须报错且不改文件。
///
/// 数据构造：三份内容，各自文件字节在安装前后必须相同。
/// 执行过程：对每份调用安装。
/// 预期结果：返回 Err；文件内容不变；负断言：内容中不出现标记。
#[test]
fn kimi_install_refuses_unmergeable_config() {
    let temp = tempfile::tempdir().expect("临时目录");
    let command_path = temp.path().join("multisoul-agent-hook");
    let cases = [
        "hooks = [{ event = \"Stop\", command = \"x\" }]\n",
        "[hooks]\nfoo = 1\n",
        "# >>> multisoul-agent-hook managed (do not edit) >>>\n[[hooks]]\n",
    ];
    for (index, body) in cases.iter().enumerate() {
        let mut paths = paths(temp.path());
        paths.kimi_config = temp.path().join(format!("case{index}.toml"));
        fs::write(&paths.kimi_config, body).unwrap();
        let result = install_hook_config(HookTarget::Kimi, &command_path, &paths);
        assert!(result.is_err(), "第 {index} 种不可合并配置必须报错");
        assert_eq!(
            fs::read_to_string(&paths.kimi_config).unwrap(),
            *body,
            "失败时不得改动用户文件"
        );
    }
}

/// 路径解析：KIMI_CODE_HOME 优先，空值回退 ~/.kimi-code；来源名可解析。
///
/// 数据构造：home = /h；KIMI_CODE_HOME = /iso、空串、未设置三种。
/// 预期结果：/iso/config.toml、/h/.kimi-code/config.toml、/h/.kimi-code/config.toml。
#[test]
fn kimi_config_path_honors_kimi_code_home() {
    let home = PathBuf::from("/h");
    assert_eq!(
        kimi_config_path_for(&home, Some(OsString::from("/iso"))),
        PathBuf::from("/iso/config.toml"),
        "KIMI_CODE_HOME 应优先"
    );
    let default = PathBuf::from("/h/.kimi-code/config.toml");
    assert_eq!(
        kimi_config_path_for(&home, Some(OsString::new())),
        default,
        "空值回退默认"
    );
    assert_eq!(kimi_config_path_for(&home, None), default, "未设置回退默认");
    assert_eq!(
        parse_hook_source("kimi"),
        Some(HookSource::Kimi),
        "来源名可解析"
    );
    assert_eq!(
        parse_hook_target("kimi"),
        Some(HookTarget::Kimi),
        "目标名可解析"
    );
    assert_eq!(parse_hook_source("nope"), None, "未知来源仍为 None");
}
