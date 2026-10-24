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
///   - 正断言：命令数组按空格连接；`bash -lc` 包装取里面的命令；fileChange 归为 Write；未知方法原样保留
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
    let wrapped = subject_of(
        &snapshot(
            "item/commandExecution/requestApproval",
            json!({"command": ["bash", "-lc", "cargo test -p app"]}),
        ),
        root,
    );
    assert_eq!(
        wrapped.command.as_deref(),
        Some("cargo test -p app"),
        "bash -lc 包装要取里面的命令"
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

/// 配置写入后再解析，得到同样的后端、策略与开关；目录不存在时会自动创建。
///
/// 数据构造：临时目录下的子目录 brain；设置为开启、Codex、质量优先。
/// 预期结果：
///   - 正断言：config.json 存在，解析结果与写入一致
///   - 负断言：写入 Claude 后端并关闭时，解析出的 enabled 为 false，不会残留上一次的值
#[test]
fn saved_settings_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("brain");
    let on = super::config::BrainSettings {
        enabled: true,
        backend: Some(BackendKind::Codex),
        strategy: Strategy::Quality,
        ..Default::default()
    };
    super::config::save_settings_to(&dir, &on).expect("应能写入配置");
    let text = std::fs::read_to_string(dir.join("config.json")).expect("配置文件应存在");
    let read = parse_settings(&text);
    assert!(read.enabled, "enabled 应保持开启");
    assert_eq!(read.backend, Some(BackendKind::Codex));
    assert_eq!(read.strategy, Strategy::Quality);
    let off = super::config::BrainSettings {
        enabled: false,
        backend: Some(BackendKind::ClaudeCode),
        strategy: Strategy::Speed,
        ..Default::default()
    };
    super::config::save_settings_to(&dir, &off).expect("应能覆盖写入");
    let read = parse_settings(&std::fs::read_to_string(dir.join("config.json")).unwrap());
    assert!(!read.enabled, "覆盖后 enabled 应为 false");
    assert_eq!(read.backend, Some(BackendKind::ClaudeCode));
}

/// `norma brain` 命令的令牌校验：没有运行中的大脑、空令牌、错误令牌都不通过，只有正确令牌通过。
///
/// 数据构造：先是没有大脑的状态；再装入令牌为 `abc` 的大脑（不启动真实服务）。
/// 预期结果：
///   - 正断言：令牌 `abc` 通过
///   - 负断言：没有大脑时任何令牌都不通过；`abd`、空字符串、`ABC` 都不通过
#[test]
fn only_the_issued_token_authorizes_brain_commands() {
    let mut state = super::BrainHostState::new();
    assert!(!state.token_matches("abc"), "没有大脑时不应放行任何令牌");
    let (_tx, status) = std::sync::mpsc::channel();
    state.host = Some(super::BrainHost {
        service: None,
        status,
        token: "abc".into(),
    });
    assert!(state.token_matches("abc"), "正确令牌应通过");
    for wrong in ["abd", "", "ABC", "abcd"] {
        assert!(!state.token_matches(wrong), "令牌 {wrong:?} 不应通过");
    }
}

/// 新增的选项（模型、定时摘要、自主权、提醒、并行）能写入再读回；缺失字段取默认值；非法模型别名被忽略。
///
/// 数据构造：主会话 opus、审批 haiku、摘要 10 分钟、关闭“写文件”和“派后续”、语音提醒、串行审批。
/// 预期结果：
///   - 正断言：读回的值与写入一致；`{}` 解析出默认值（摘要 5 分钟、三项自主权全开、仅概览提醒、并行）
///   - 负断言：模型写成 "gpt-9" 时读回 None，不会把任意字符串当作模型别名传给后端
#[test]
fn extended_settings_round_trip_and_defaults() {
    use super::config::{Autonomy, SlowReminder, SummaryEvery};
    let root = tempfile::tempdir().unwrap();
    let custom = super::config::BrainSettings {
        enabled: true,
        backend: Some(BackendKind::ClaudeCode),
        strategy: Strategy::Auto,
        main_model: Some("opus".into()),
        approval_model: Some("haiku".into()),
        summary: SummaryEvery::TenMinutes,
        autonomy: Autonomy {
            readonly_commands: true,
            project_writes: false,
            assign_followups: false,
        },
        slow_reminder: SlowReminder::VoiceAfter60s,
        parallel_approvals: false,
        speak: false,
        always_ask: vec!["Bash:cargo".into(), "Write".into()],
    };
    super::config::save_settings_to(root.path(), &custom).expect("应能写入");
    let read = parse_settings(&std::fs::read_to_string(root.path().join("config.json")).unwrap());
    assert_eq!(read, custom, "写入后读回应完全一致");
    let defaults = parse_settings("{}");
    assert_eq!(defaults.summary, SummaryEvery::FiveMinutes);
    assert_eq!(
        defaults.autonomy,
        Autonomy {
            readonly_commands: true,
            project_writes: true,
            assign_followups: true
        }
    );
    assert_eq!(defaults.slow_reminder, SlowReminder::OverviewOnly);
    assert!(defaults.parallel_approvals, "默认并行处理审批");
    assert!(defaults.speak, "默认出声汇报");
    assert_eq!(
        parse_settings(r#"{"main_model":"gpt-9"}"#).main_model,
        None,
        "非法别名要忽略"
    );
}

/// 自主权开关按操作类别生效：命令看“只读、测试、构建”，写文件看“项目内写文件”，读类工具不受限。
///
/// 数据构造：只读命令开、写文件关；以及相反的组合。
/// 预期结果：
///   - 正断言：Bash 随 readonly_commands，Write/Edit 随 project_writes
///   - 负断言：关掉对应开关后，该类工具不再被允许；Read 不受任何开关影响
#[test]
fn autonomy_switches_apply_per_tool_category() {
    use super::approvals::autonomy_allows;
    use super::config::Autonomy;
    let commands_only = Autonomy {
        readonly_commands: true,
        project_writes: false,
        assign_followups: true,
    };
    assert!(
        autonomy_allows(&commands_only, "Bash"),
        "命令开关打开时应允许"
    );
    assert!(
        !autonomy_allows(&commands_only, "Write"),
        "写文件开关关闭时不应允许 Write"
    );
    assert!(
        !autonomy_allows(&commands_only, "Edit"),
        "写文件开关关闭时不应允许 Edit"
    );
    let writes_only = Autonomy {
        readonly_commands: false,
        project_writes: true,
        assign_followups: true,
    };
    assert!(
        !autonomy_allows(&writes_only, "Bash"),
        "命令开关关闭时不应允许 Bash"
    );
    assert!(autonomy_allows(&writes_only, "MultiEdit"));
    assert!(autonomy_allows(
        &Autonomy {
            readonly_commands: false,
            project_writes: false,
            assign_followups: false
        },
        "Read"
    ));
}

/// “以后这类先问我”的类别键：命令按程序名（忽略路径和大小写），写文件统一成 Write，其他工具按工具名。
///
/// 数据构造：`cargo test`、`/usr/bin/CARGO build`、`git status`；Write 与 Edit；一个 MCP 工具。
/// 预期结果：
///   - 正断言：前两个都是 `Bash:cargo`；Write 与 Edit 都是 `Write`；MCP 工具保持原名
///   - 负断言：`git status` 的键不等于 cargo 的键，避免“先问 cargo”误伤 git
#[test]
fn ask_keys_group_commands_by_program_and_writes_together() {
    use super::ask_key;
    let subject = |tool: &str, command: Option<&str>| brain::risk::ApprovalSubject {
        tool_name: tool.into(),
        command: command.map(str::to_string),
        file_path: None,
        project_root: "/p".into(),
    };
    assert_eq!(
        ask_key(&subject("Bash", Some("cargo test -p app"))),
        "Bash:cargo"
    );
    assert_eq!(
        ask_key(&subject("Bash", Some("/usr/bin/CARGO build"))),
        "Bash:cargo"
    );
    assert_ne!(ask_key(&subject("Bash", Some("git status"))), "Bash:cargo");
    assert_eq!(ask_key(&subject("Write", None)), "Write");
    assert_eq!(ask_key(&subject("Edit", None)), "Write");
    assert_eq!(ask_key(&subject("mcp__x__y", None)), "mcp__x__y");
}

/// 自主处理记录最多保留 20 条，新的在前，每条有递增的标识。
///
/// 数据构造：连续放入 25 条。
/// 预期结果：
///   - 正断言：只剩 20 条；第一条是最后放入的（标识 25）；标识严格递减排列
///   - 负断言：最早的 5 条（标识 1 到 5）已被丢弃
#[test]
fn the_autonomy_log_keeps_the_newest_twenty() {
    use super::log::AutoLog;
    use super::{AutoAction, AutoKind};
    let mut log = AutoLog::default();
    for n in 0..25 {
        log.push(AutoAction {
            id: 0,
            at: std::time::SystemTime::now(),
            kind: AutoKind::Approved,
            agent: "Claude".into(),
            node_id: 1,
            summary: format!("事项 {n}"),
            reason: "r".into(),
            ask_key: "Bash:cargo".into(),
        });
    }
    let items = log.items();
    assert_eq!(items.len(), 20, "最多 20 条");
    assert_eq!(items[0].id, 25, "最新的排最前");
    assert!(
        items.windows(2).all(|pair| pair[0].id > pair[1].id),
        "标识应严格递减"
    );
    assert!(items.iter().all(|item| item.id > 5), "最早的 5 条应被丢弃");
}
