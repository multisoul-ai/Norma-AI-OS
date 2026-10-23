use super::approvals::{encode_id, subject_of, summarize};
use super::config::parse_settings;
use brain::{BackendKind, Strategy};
use norma::mobile::{ApprovalSnapshot, ApprovalTarget};
use serde_json::json;
use std::path::Path;

fn target() -> ApprovalTarget {
    ApprovalTarget {
        node_id: 7,
        profile: "Claude".into(),
        run_id: "run-1".into(),
        session_id: "sess-1".into(),
        turn_id: "turn-1".into(),
        request_id: "req-1".into(),
    }
}

fn snapshot(tool: &str, details: serde_json::Value) -> ApprovalSnapshot {
    ApprovalSnapshot {
        target: target(),
        card_title: "设置页".into(),
        tool: tool.into(),
        details,
        decisions: vec![],
    }
}

/// 审批标识由六个字段按 `.` 连接，字段本身不含 `.`，所以标识稳定且可读。
///
/// 数据构造：node 7、Claude、run-1、sess-1、turn-1、req-1。
/// 预期结果：
///   - 正断言：标识等于 `7.Claude.run-1.sess-1.turn-1.req-1`
///   - 负断言：node 不同时标识不同，不会把两张卡片的审批混为一条
#[test]
fn approval_ids_are_stable_and_distinct_per_node() {
    assert_eq!(
        encode_id(&target()),
        "7.Claude.run-1.sess-1.turn-1.req-1",
        "标识格式"
    );
    let other = ApprovalTarget {
        node_id: 8,
        ..target()
    };
    assert_ne!(
        encode_id(&target()),
        encode_id(&other),
        "不同 node 的审批标识必须不同"
    );
}

/// Claude 的审批：tool 是工具名，details 是 tool_input。
///
/// 数据构造：Bash + {command}；Write + {file_path}。
/// 预期结果：
///   - 正断言：Bash 取到命令，Write 取到路径；项目根带上
///   - 负断言：Bash 没有 file_path，Write 没有 command
#[test]
fn claude_approvals_map_tool_input_to_a_subject() {
    let root = Path::new("/work/proj");
    let bash = subject_of(
        &snapshot("Bash", json!({"command": "cargo test -p app"})),
        root,
    );
    assert_eq!(bash.tool_name, "Bash");
    assert_eq!(bash.command.as_deref(), Some("cargo test -p app"));
    assert!(bash.file_path.is_none(), "Bash 不应有路径");
    assert_eq!(bash.project_root, root);
    let write = subject_of(
        &snapshot("Write", json!({"file_path": "/work/proj/a.rs"})),
        root,
    );
    assert_eq!(
        write.file_path.as_deref(),
        Some(Path::new("/work/proj/a.rs"))
    );
    assert!(write.command.is_none(), "Write 不应有命令");
}

/// Codex 的审批：tool 是请求方法名，命令可能是数组；无法识别的方法保持原名，由规则按未知工具处理。
///
/// 数据构造：commandExecution + command 数组；fileChange；未知方法。
/// 预期结果：
///   - 正断言：命令数组按空格连接；fileChange 归为 Write；未知方法原样保留
///   - 负断言：未知方法不会被当成 Bash
#[test]
fn codex_approvals_map_method_names_to_tools() {
    let root = Path::new("/work/proj");
    let exec = subject_of(
        &snapshot(
            "item/commandExecution/requestApproval",
            json!({"command": ["cargo", "test"]}),
        ),
        root,
    );
    assert_eq!(
        (exec.tool_name.as_str(), exec.command.as_deref()),
        ("Bash", Some("cargo test"))
    );
    let patch = subject_of(
        &snapshot(
            "item/fileChange/requestApproval",
            json!({"path": "src/a.rs"}),
        ),
        root,
    );
    assert_eq!(patch.tool_name, "Write");
    let unknown = subject_of(&snapshot("item/mystery/requestApproval", json!({})), root);
    assert_eq!(
        unknown.tool_name, "item/mystery/requestApproval",
        "未知方法保持原名"
    );
    assert_ne!(unknown.tool_name, "Bash");
}

/// 播报用的摘要最多 60 个字符，过长加省略号。
///
/// 数据构造：一条 80 字符的命令。
/// 预期结果：
///   - 正断言：摘要是前 60 个字符加 `…`
///   - 负断言：短命令不加省略号
#[test]
fn summaries_are_clipped_to_sixty_characters() {
    let root = Path::new("/p");
    let long = subject_of(&snapshot("Bash", json!({"command": "a".repeat(80)})), root);
    assert_eq!(
        summarize(&long),
        format!("{}…", "a".repeat(60)),
        "超过 60 字符应截断"
    );
    let short = subject_of(&snapshot("Bash", json!({"command": "ls"})), root);
    assert_eq!(summarize(&short), "ls", "短命令原样返回");
}

/// 配置解析：缺失、损坏、关闭都不启动大脑；正常配置读出后端和策略。
///
/// 预期结果：
///   - 正断言：`{"enabled":true,"backend":"claude","strategy":"speed"}` 读出 Claude Code 与 Speed
///   - 负断言：空文本、非 JSON、没有 enabled 的配置都是关闭；未知后端为 None
#[test]
fn settings_parse_defaults_to_disabled() {
    let on = parse_settings(r#"{"enabled":true,"backend":"claude","strategy":"speed"}"#);
    assert!(
        on.enabled && on.backend == Some(BackendKind::ClaudeCode) && on.strategy == Strategy::Speed
    );
    for text in ["", "not json", "{}", r#"{"backend":"claude"}"#] {
        assert!(!parse_settings(text).enabled, "{text:?} 应视为关闭");
    }
    assert_eq!(
        parse_settings(r#"{"enabled":true,"backend":"mystery"}"#).backend,
        None,
        "未知后端不选"
    );
}
