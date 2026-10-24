use super::*;
use std::fs;

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
