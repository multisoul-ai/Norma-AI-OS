use crate::agent_hook::{
    agent_hooks_installed_for_source, hook_events_for_source, install_hook_config,
    parse_hook_event, task_state_for_provider_event, uninstall_hook_config, AgentHookTaskState,
    HookInstallPaths, HookSource, HookTarget,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn paths_in(dir: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: dir.join("claude.json"),
        codex_hooks: dir.join("codex.json"),
        cursor_hooks: dir.join("cursor.json"),
        grok_hooks: dir.join("grok.json"),
        opencode_plugin: dir.join("opencode.js"),
        infcode_settings: dir.join("infcode-settings.json"),
        zcode_config: dir.join("zcode").join("config.json"),
        pi_extension: dir.join("pi-agent/extensions/norma-agent-hook.ts"),
        kimi_config: dir.join("kimi-home").join("config.toml"),
        hermes_plugin_dir: dir.join("hermes/plugins/norma-agent-hook"),
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("读取配置")).expect("合法 JSON")
}

/// 场景：全新环境安装。执行 install_hook_config(ZCode)，预期 7 个已验证事件各有 1 条 Norma 命令，
/// 并且 hooks.enabled=true（实测缺省关闭，不设则零事件）。
#[test]
fn install_writes_seven_verified_events_and_enables_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    install_hook_config(
        HookTarget::ZCode,
        Path::new("/opt/Norma OS/multisoul-agent-hook"),
        &paths,
    )
    .unwrap();
    let root = read(&paths.zcode_config);
    assert_eq!(
        hook_events_for_source(HookSource::ZCode).len(),
        7,
        "实测恰好 7 个事件"
    );
    for event in hook_events_for_source(HookSource::ZCode) {
        let groups = root["hooks"]["events"][event].as_array().expect("事件数组");
        assert_eq!(groups.len(), 1, "{event} 应恰好一条");
        let command = groups[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            command.ends_with("forward --source zcode"),
            "命令: {command}"
        );
        assert_eq!(groups[0]["hooks"][0]["type"], "command");
    }
    assert_eq!(root["hooks"]["enabled"], json!(true), "必须开启 hooks");
    assert!(
        agent_hooks_installed_for_source(HookSource::ZCode, &paths),
        "应判定为已安装"
    );
}

/// 场景：用户配置含自有模型、未知字段与自有 hook。安装两次（幂等）再卸载，
/// 预期用户字段与用户 hook 保留，Norma 项清空，且不回写 enabled。
#[test]
fn install_is_idempotent_and_uninstall_keeps_user_data() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::create_dir_all(paths.zcode_config.parent().unwrap()).unwrap();
    let user = json!({
        "model": "m", "userKeep": {"x": 1},
        "hooks": {"enabled": true, "events": {"Stop": [{"hooks": [{"type": "command", "command": "echo mine"}]}]}}
    });
    std::fs::write(&paths.zcode_config, user.to_string()).unwrap();
    let bin = PathBuf::from("/a/multisoul-agent-hook");
    install_hook_config(HookTarget::ZCode, &bin, &paths).unwrap();
    install_hook_config(HookTarget::ZCode, &bin, &paths).unwrap();
    let root = read(&paths.zcode_config);
    assert_eq!(
        root["hooks"]["events"]["Stop"].as_array().unwrap().len(),
        2,
        "用户项 + 1 条 Norma，重复安装不叠加"
    );
    assert_eq!(root["userKeep"]["x"], 1, "未知字段保留");

    uninstall_hook_config(HookTarget::ZCode, &paths).unwrap();
    let root = read(&paths.zcode_config);
    assert!(
        !agent_hooks_installed_for_source(HookSource::ZCode, &paths),
        "卸载后不再已安装"
    );
    assert_eq!(
        root["hooks"]["events"]["Stop"][0]["hooks"][0]["command"], "echo mine",
        "用户 hook 保留"
    );
    assert_eq!(
        root["hooks"]["events"]["PreToolUse"],
        json!([]),
        "Norma 项被清空后事件数组保留为空"
    );
    assert_eq!(root["model"], "m");
}

/// 场景：配置文件不是合法 JSON 或 hooks.events 不是对象。预期安装返回错误且原文件字节不变（拒绝覆盖）。
#[test]
fn install_refuses_to_overwrite_foreign_or_malformed_config() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::create_dir_all(paths.zcode_config.parent().unwrap()).unwrap();
    for original in [
        "not json {",
        r#"{"hooks":{"events":[1,2]}}"#,
        r#"{"hooks":5}"#,
    ] {
        std::fs::write(&paths.zcode_config, original).unwrap();
        let result = install_hook_config(
            HookTarget::ZCode,
            Path::new("/a/multisoul-agent-hook"),
            &paths,
        );
        assert!(result.is_err(), "应拒绝: {original}");
        assert_eq!(
            std::fs::read_to_string(&paths.zcode_config).unwrap(),
            original,
            "原文件不得被改写"
        );
    }
}

/// 场景：只有 hooks.events 完整但 enabled 缺省。预期不算已安装（否则 ZCode 不会触发事件）。
#[test]
fn installed_requires_enabled_flag() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    install_hook_config(
        HookTarget::ZCode,
        Path::new("/a/multisoul-agent-hook"),
        &paths,
    )
    .unwrap();
    let mut root = read(&paths.zcode_config);
    root["hooks"].as_object_mut().unwrap().remove("enabled");
    std::fs::write(&paths.zcode_config, root.to_string()).unwrap();
    assert!(
        !agent_hooks_installed_for_source(HookSource::ZCode, &paths),
        "enabled 缺省视为未安装"
    );
}

/// 场景：真实载荷夹具（camelCase：hookEventName/sessionId/toolName）。预期会话 ID、工具名被解析，
/// 并映射到五状态：SessionStart->Ready，PreToolUse->RunningTool，PermissionRequest->WaitingApproval，
/// PostToolUseFailure->Thinking，Stop->Done。
#[test]
fn real_payload_fixtures_map_to_states() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/research/evidence/2026-10-18-zcode/hook-payloads");
    let cases = [
        ("SessionStart.headless-yolo.json", AgentHookTaskState::Ready),
        (
            "UserPromptSubmit.headless-yolo.json",
            AgentHookTaskState::Thinking,
        ),
        (
            "PreToolUse.tui-build.json",
            AgentHookTaskState::RunningTool {
                tool_name: Some("Bash".into()),
            },
        ),
        (
            "PermissionRequest.tui-build-approval.json",
            AgentHookTaskState::WaitingApproval {
                tool_name: Some("Bash".into()),
            },
        ),
        ("PostToolUse.tui-build.json", AgentHookTaskState::Thinking),
        (
            "PostToolUseFailure.tui-build.json",
            AgentHookTaskState::Thinking,
        ),
        ("Stop.headless-yolo.json", AgentHookTaskState::Done),
    ];
    for (file, expected) in cases {
        let raw: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(file)).unwrap()).unwrap();
        let event = parse_hook_event(HookSource::ZCode, 9, raw).unwrap();
        assert!(
            event
                .session_id
                .as_deref()
                .is_some_and(|id| id.starts_with("sess_")),
            "{file} 会话 ID"
        );
        assert_eq!(
            task_state_for_provider_event(&event),
            Some(expected),
            "{file} 状态"
        );
    }
}

/// 场景：用户分组形态各异（缺 hooks 字段、hooks 为空数组），且 Stop 下另有一条 Norma 项。
/// 数据构造：Stop = [{matcher:x}, {hooks:[]}, Norma 分组]，PreToolUse 为用户空数组 []；hooks.enabled=true。
/// 执行：卸载。预期：Stop 保留前两个用户分组并只去掉 Norma；PreToolUse 保持 []。
#[test]
fn uninstall_keeps_user_owned_empty_groups_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::create_dir_all(paths.zcode_config.parent().unwrap()).unwrap();
    let user = json!({"hooks": {"enabled": true, "events": {
        "Stop": [{"matcher": "x"}, {"hooks": []}],
        "PreToolUse": []
    }}});
    std::fs::write(&paths.zcode_config, user.to_string()).unwrap();
    install_hook_config(
        HookTarget::ZCode,
        Path::new("/a/multisoul-agent-hook"),
        &paths,
    )
    .unwrap();
    uninstall_hook_config(HookTarget::ZCode, &paths).unwrap();
    let root = read(&paths.zcode_config);
    assert_eq!(
        root["hooks"]["events"]["Stop"],
        json!([{"matcher": "x"}, {"hooks": []}]),
        "用户分组须原样保留"
    );
    assert_eq!(
        root["hooks"]["events"]["PreToolUse"],
        json!([]),
        "用户原有的空事件数组不得被删"
    );
    assert_eq!(
        root["hooks"]["events"]["SessionStart"],
        json!([]),
        "Norma 创建的事件清空后保留空数组，不误删可能属于用户的键"
    );
}

/// 场景：用户已有 hook 但 hooks.enabled=false（休眠）。预期安装拒绝且文件字节不变；
/// 无用户 hook 时 enabled 缺省仍可正常安装。
#[test]
fn install_refuses_when_dormant_user_hooks_would_be_activated() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::create_dir_all(paths.zcode_config.parent().unwrap()).unwrap();
    let original = r#"{"hooks":{"enabled":false,"events":{"Stop":[{"hooks":[{"type":"command","command":"echo mine"}]}]}}}"#;
    std::fs::write(&paths.zcode_config, original).unwrap();
    let bin = Path::new("/a/multisoul-agent-hook");
    assert!(
        install_hook_config(HookTarget::ZCode, bin, &paths).is_err(),
        "休眠的用户 hook 不得被安装悄悄激活"
    );
    assert_eq!(
        std::fs::read_to_string(&paths.zcode_config).unwrap(),
        original,
        "拒绝时文件不变"
    );
    std::fs::write(&paths.zcode_config, r#"{"model":"m"}"#).unwrap();
    install_hook_config(HookTarget::ZCode, bin, &paths).expect("无用户 hook 时应可安装");
}
