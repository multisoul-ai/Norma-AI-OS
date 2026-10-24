use super::*;
use std::fs;

/// 内置只读工具也必须经过权限请求，不能递归读取整个项目绕过逐文件检查。
/// 数据构造：Claude 会话配置，不包含任何用户设置。
/// 执行过程：检查工具白名单、逐次询问规则和跳过权限开关。
/// 预期结果：只暴露 Bash/Read，每次都询问，禁用跳过权限与额外 MCP 配置。
#[test]
fn claude_requires_permission_for_every_exposed_tool() {
    let meta = session_meta(BackendKind::ClaudeCode, None).expect("应有 Claude 配置");
    let options = &meta["claudeCode"]["options"];
    assert_eq!(
        options["tools"],
        serde_json::json!(["Bash", "Read"]),
        "不能暴露递归搜索或其他工具"
    );
    assert_eq!(
        options["settings"]["permissions"]["ask"],
        serde_json::json!(["Bash", "Read"]),
        "内置安全规则也必须转到 ACP 询问"
    );
    assert_eq!(
        options["allowDangerouslySkipPermissions"], false,
        "不能允许跳过权限"
    );
    assert_eq!(options["strictMcpConfig"], true, "不能加载用户的 MCP 工具");
}

/// 在适配器目录和 PATH 中检测后端，并保持固定顺序。
///
/// 数据构造：临时目录里放 node_modules/.bin/claude-agent-acp，PATH 目录里放 opencode。
/// 预期结果：
///   - 正断言：检测到 Claude Code（无参数）和 OpenCode（参数 acp）
///   - 负断言：没有 codex-acp 时不会出现 Codex
#[test]
fn detects_installed_backends() {
    let adapters = tempfile::tempdir().unwrap();
    let path_dir = tempfile::tempdir().unwrap();
    let bin = adapters.path().join("node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("claude-agent-acp"), "").unwrap();
    fs::write(path_dir.path().join("opencode"), "").unwrap();
    let found = detect(adapters.path(), &[path_dir.path().to_path_buf()]);
    let kinds: Vec<_> = found.iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        vec![BackendKind::ClaudeCode, BackendKind::OpenCode],
        "应只检测到两个后端"
    );
    assert_eq!(
        found[1].args,
        vec!["acp".to_string()],
        "OpenCode 要以 acp 子命令启动"
    );
}

/// 策略映射到模型；非 Claude 后端不指定模型。
///
/// 预期结果：
///   - 正断言：自动策略主会话 opus、审批 haiku；速度优先两者都是 haiku
///   - 负断言：Codex 与 OpenCode 的模型都是 None
#[test]
fn strategies_map_to_models() {
    let auto = models_for(BackendKind::ClaudeCode, Strategy::Auto);
    assert_eq!(
        (auto.main.as_deref(), auto.approval.as_deref()),
        (Some("opus"), Some("haiku"))
    );
    let speed = models_for(BackendKind::ClaudeCode, Strategy::Speed);
    assert_eq!(
        (speed.main.as_deref(), speed.approval.as_deref()),
        (Some("haiku"), Some("haiku"))
    );
    for kind in [BackendKind::Codex, BackendKind::OpenCode] {
        assert_eq!(
            models_for(kind, Strategy::Quality),
            Models {
                main: None,
                approval: None
            },
            "{kind:?} 不指定模型"
        );
    }
}

/// Claude Code 会话必须忽略用户自己的设置并禁用写工具，否则用户的 bypassPermissions 会让闸门失效。
///
/// 预期结果：
///   - 正断言：meta 里 settingSources 为空数组，disallowedTools 含 Write 与 Edit，带上模型；会话模式为 default
///   - 负断言：其他后端没有 meta 与模式
#[test]
fn claude_sessions_ignore_user_settings_and_disable_write_tools() {
    let meta = session_meta(BackendKind::ClaudeCode, Some("haiku")).expect("Claude 需要 meta");
    let options = &meta["claudeCode"]["options"];
    assert_eq!(
        options["settingSources"],
        serde_json::json!([]),
        "不能加载用户的权限设置"
    );
    let denied = options["disallowedTools"].as_array().unwrap();
    assert!(
        denied.iter().any(|t| t == "Write") && denied.iter().any(|t| t == "Edit"),
        "写工具要禁用"
    );
    assert_eq!(options["model"], "haiku");
    assert_eq!(session_mode(BackendKind::ClaudeCode), Some("default"));
    // Codex 默认的 Auto review 模式跑命令不发权限请求，闸门会失效；只读模式下写入和连 socket 都要请求批准。
    assert_eq!(session_mode(BackendKind::Codex), Some("read-only"));
    assert!(
        session_meta(BackendKind::Codex, None).is_none()
            && session_mode(BackendKind::OpenCode).is_none()
    );
}

/// OpenCode 候选配置包含 ask/deny 规则；这里只验证序列化，不证明后端遵守规则。
/// 服务层在完成实际协议和权限验证前拒绝启动该后端。
///
/// 预期结果：
///   - 正断言：OpenCode 的环境变量里有 OPENCODE_CONFIG_CONTENT，bash 为 ask，edit 与 webfetch 为 deny
///   - 负断言：其他后端不注入环境变量
#[test]
fn opencode_configuration_contains_requested_permissions() {
    let env = session_env(BackendKind::OpenCode);
    let (_, content) = env
        .iter()
        .find(|(key, _)| key == "OPENCODE_CONFIG_CONTENT")
        .expect("要注入 OpenCode 配置");
    let config: serde_json::Value = serde_json::from_str(content).expect("应是 JSON");
    assert_eq!(config["permission"]["bash"], "ask");
    assert_eq!(config["permission"]["edit"], "deny");
    assert_eq!(config["permission"]["webfetch"], "deny");
    assert!(session_env(BackendKind::ClaudeCode).is_empty());
    assert!(session_env(BackendKind::Codex).is_empty());
}

/// Codex 的策略对应推理强度：主会话按速度、自动、质量取 low、medium、high，审批会话始终 low。
///
/// 预期结果：
///   - 正断言：Codex 各档取值如上
///   - 负断言：Claude Code 与 OpenCode 不设推理强度
#[test]
fn codex_strategy_maps_to_reasoning_effort() {
    let main = |s| effort_for(BackendKind::Codex, s, true);
    assert_eq!(main(Strategy::Speed), Some("low"));
    assert_eq!(main(Strategy::Auto), Some("medium"));
    assert_eq!(main(Strategy::Quality), Some("high"));
    assert_eq!(
        effort_for(BackendKind::Codex, Strategy::Quality, false),
        Some("low")
    );
    assert_eq!(
        effort_for(BackendKind::ClaudeCode, Strategy::Quality, true),
        None
    );
    assert_eq!(
        effort_for(BackendKind::OpenCode, Strategy::Auto, true),
        None
    );
}

/// 从 session/new 的 configOptions 里读出模型列表：取 id 为 model 的那一项。
///
/// 数据构造：仿 Codex 的回复，含 mode 与 model 两项；另给一个没有 model 项的回复。
/// 预期结果：
///   - 正断言：得到 (值, 名称) 列表，顺序不变；没有名称时用值
///   - 负断言：mode 的选项不会混进来；没有 model 项时为空
#[test]
fn model_choices_come_from_the_model_config_option() {
    let created = serde_json::json!({
        "sessionId": "s",
        "configOptions": [
            {"id": "mode", "options": [{"value": "read-only", "name": "Read-only"}]},
            {"id": "model", "currentValue": "gpt-6-astra", "options": [
                {"value": "gpt-6.1-sol", "name": "6.1 Sol"},
                {"value": "gpt-6-astra"}
            ]}
        ]
    });
    assert_eq!(
        model_choices(&created),
        vec![
            ModelChoice {
                id: "gpt-6.1-sol".into(),
                name: "6.1 Sol".into()
            },
            ModelChoice {
                id: "gpt-6-astra".into(),
                name: "gpt-6-astra".into()
            },
        ]
    );
    assert!(model_choices(&serde_json::json!({"sessionId": "s"})).is_empty());
}
