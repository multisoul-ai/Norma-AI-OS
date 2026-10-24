use super::*;
use serde_json::json;

fn gate() -> Gate {
    Gate {
        norma_bin: "/opt/norma/bin/norma".into(),
        project_root: "/work/proj".into(),
    }
}

fn bash(command: &str) -> PermissionRequest {
    PermissionRequest {
        tool: "Bash".into(),
        command: Some(command.into()),
        path: None,
    }
}

fn read(path: &str) -> PermissionRequest {
    PermissionRequest {
        tool: "Read".into(),
        command: None,
        path: Some(path.into()),
    }
}

/// 只放行恰好一条、由 norma 可执行文件发起的 brain 子命令。
///
/// 数据构造：norma 路径 /opt/norma/bin/norma。
/// 执行过程：判断四条命令。
/// 预期结果：
///   - 正断言：`norma brain agents` 放行
///   - 负断言：管道、组合、重定向、card send 与 agent task 等其他子命令、其他可执行文件、同名但路径不同的 norma 都被拒绝
#[test]
fn only_a_single_norma_command_is_allowed() {
    let g = gate();
    assert_eq!(
        g.judge(&bash("/opt/norma/bin/norma brain agents")),
        Verdict::Allow
    );
    for cmd in [
        "/opt/norma/bin/norma brain agents | head",
        "/opt/norma/bin/norma brain agents && rm -rf x",
        "/opt/norma/bin/norma brain report --text hi > /etc/x",
        "/opt/norma/bin/norma card send 3 \"rm -rf ~\" --submit",
        "/opt/norma/bin/norma agent task assign X --task y",
        "rm -rf x",
        "/tmp/evil/norma brain agents",
        "echo $(whoami)",
    ] {
        assert!(
            matches!(g.judge(&bash(cmd)), Verdict::Reject(_)),
            "{cmd} 应被拒绝"
        );
    }
}

/// 闸门不得把输入、输出或文件描述符重定向当成普通参数。
/// 数据构造：同一条合法 agents 命令，分别附加四类 shell 重定向。
/// 执行过程：逐条经 Gate 判断，并以没有重定向的命令作对照。
/// 预期结果：普通命令放行，所有重定向拒绝，避免 shell 在 CLI 执行前截断文件。
#[test]
fn every_shell_redirection_is_rejected() {
    let g = gate();
    assert_eq!(
        g.judge(&bash("/opt/norma/bin/norma brain agents")),
        Verdict::Allow,
        "没有重定向的命令仍应放行"
    );
    for suffix in [
        ">&/tmp/fixture",
        "2>&1",
        "<&0",
        "< /tmp/fixture",
        "&>/tmp/fixture",
        "|",
        "&",
        ";",
        "&&",
    ] {
        let command = format!("/opt/norma/bin/norma brain agents {suffix}");
        assert!(
            matches!(g.judge(&bash(&command)), Verdict::Reject(_)),
            "重定向必须被拒绝：{command}"
        );
    }
}

/// 读取只限工作空间内，且不能读密钥。
///
/// 数据构造：项目根 /work/proj。
/// 预期结果：
///   - 正断言：/work/proj/docs/a.md 和相对路径 src/a.rs 放行
///   - 负断言：/etc/hosts、`../x`、/work/proj/.env、缺路径都被拒绝
#[test]
fn reads_are_limited_to_the_project_and_never_secrets() {
    let dir = tempfile::tempdir().expect("应创建项目夹具");
    std::fs::create_dir_all(dir.path().join("src")).expect("应创建源码目录");
    std::fs::write(dir.path().join("src/a.rs"), "fixture").expect("应创建项目文件");
    let g = Gate {
        project_root: dir.path().to_path_buf(),
        ..gate()
    };
    assert_eq!(
        g.judge(&read("src/a.rs")),
        Verdict::Allow,
        "项目内实际文件应放行"
    );
    for path in ["/etc/hosts", "../x", "/work/proj/.env"] {
        assert!(
            matches!(g.judge(&read(path)), Verdict::Reject(_)),
            "{path} 应被拒绝"
        );
    }
    let missing = PermissionRequest {
        tool: "Read".into(),
        command: None,
        path: None,
    };
    assert!(
        matches!(g.judge(&missing), Verdict::Reject(_)),
        "缺路径应被拒绝"
    );
}

/// 项目内的普通名称不能通过符号链接读取项目外文件或项目内凭据。
/// 数据构造：真实项目文件、项目外文件及 .env，再为后两者创建无敏感字样的链接。
/// 执行过程：分别请求真实文件和两个链接。
/// 预期结果：真实文件放行，两个伪装链接及不存在的路径拒绝。
#[test]
fn symlinked_and_missing_reads_are_rejected() {
    let dir = tempfile::tempdir().expect("应创建夹具目录");
    let root = dir.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    std::fs::write(root.join("safe.txt"), "safe").expect("应创建正常文件");
    std::fs::write(root.join(".env"), "fake").expect("应创建伪凭据");
    std::fs::write(dir.path().join("outside.txt"), "outside").expect("应创建项目外夹具");
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), root.join("outside-link"))
        .expect("应创建越界链接");
    std::os::unix::fs::symlink(root.join(".env"), root.join("innocent.txt"))
        .expect("应创建凭据链接");
    let g = Gate {
        project_root: root,
        ..gate()
    };
    assert_eq!(g.judge(&read("safe.txt")), Verdict::Allow, "正常文件应放行");
    for path in ["outside-link", "innocent.txt", "missing.txt"] {
        assert!(
            matches!(g.judge(&read(path)), Verdict::Reject(_)),
            "不能放行 {path}"
        );
    }
}

/// 目录授权不能覆盖目录内的凭据、链接或任意搜索目标。
/// 数据构造：项目根内有正常源码；四种工具分别请求读取根目录。
/// 执行过程：读取正常文件作正对照，再判断目录与递归工具请求。
/// 预期结果：只允许逐个普通文件的 Read，不允许目录或搜索工具。
#[test]
fn directory_and_recursive_reads_are_rejected() {
    let dir = tempfile::tempdir().expect("应创建项目");
    std::fs::write(dir.path().join("safe.txt"), "safe").expect("应写入正常文件");
    let g = Gate {
        project_root: dir.path().into(),
        ..gate()
    };
    assert_eq!(
        g.judge(&read("safe.txt")),
        Verdict::Allow,
        "逐文件读取正常文件应放行"
    );
    for (tool, path) in [
        ("Read", "."),
        ("Glob", "."),
        ("Grep", "safe.txt"),
        ("LS", "."),
    ] {
        let request = PermissionRequest {
            tool: tool.into(),
            path: Some(path.into()),
            command: None,
        };
        assert!(
            matches!(g.judge(&request), Verdict::Reject(_)),
            "不能授予目录或递归访问：{request:?}"
        );
    }
}

/// 写类工具和未知工具一律拒绝。
///
/// 数据构造：Write、Edit、mcp 工具。
/// 预期结果：三者都是 Reject。
#[test]
fn write_and_unknown_tools_are_rejected() {
    let g = gate();
    for tool in ["Write", "Edit", "mcp__github__x"] {
        let request = PermissionRequest {
            tool: tool.into(),
            command: None,
            path: Some("/work/proj/a".into()),
        };
        assert!(
            matches!(g.judge(&request), Verdict::Reject(_)),
            "{tool} 应被拒绝"
        );
    }
}

/// 从不同适配器的权限请求里提取字段，并挑出对应的选项。
///
/// 数据构造：Claude 形态（name + rawInput.command）；Codex 形态（只有 kind=execute，command 是数组）。
/// 预期结果：
///   - 正断言：两种都解析为 Bash 和完整命令；放行选 allow-once，拒绝选 reject
///   - 负断言：没有任何 allow 选项时放行返回 None
#[test]
fn permission_requests_are_parsed_from_both_adapter_shapes() {
    let claude = json!({
        "toolCall": {"name": "Bash", "kind": "execute", "rawInput": {"command": "norma brain report"}},
        "options": [{"optionId": "allow-once", "kind": "allow_once"}, {"optionId": "reject", "kind": "reject_once"}]
    });
    let codex = json!({
        "toolCall": {"kind": "execute", "rawInput": {"command": ["bash", "-lc", "norma brain report"]}},
        "options": [{"optionId": "approved", "kind": "allow_once"}, {"optionId": "abort", "kind": "reject_once"}]
    });
    let a = PermissionRequest::from_params(&claude);
    assert_eq!(
        (a.tool.as_str(), a.command.as_deref()),
        ("Bash", Some("norma brain report"))
    );
    let b = PermissionRequest::from_params(&codex);
    assert_eq!(
        (b.tool.as_str(), b.command.as_deref()),
        ("Bash", Some("'bash' '-lc' 'norma brain report'")),
        "argv 边界必须通过引号保留"
    );
    assert_eq!(
        pick_option(&claude, &Verdict::Allow).as_deref(),
        Some("allow-once")
    );
    assert_eq!(
        pick_option(&claude, &Verdict::Reject("x".into())).as_deref(),
        Some("reject")
    );
    assert_eq!(
        pick_option(&codex, &Verdict::Allow).as_deref(),
        Some("approved")
    );
    let none = json!({"options": [{"optionId": "reject", "kind": "reject_once"}]});
    assert_eq!(
        pick_option(&none, &Verdict::Allow),
        None,
        "没有 allow 选项时不能硬选"
    );
}

/// Codex 适配器的权限请求：没有 name，kind 为 execute，命令是字符串并附带 cwd；
/// 中文文字和全角标点放在单引号里，仍是一条 norma brain 命令。
///
/// 数据构造：照搬 codex-acp 2.2.2 实际发来的 report 请求。
/// 预期结果：
///   - 正断言：放行
///   - 负断言：无
#[test]
fn codex_report_request_is_allowed() {
    let params = json!({
        "sessionId": "s",
        "toolCall": {
            "toolCallId": "exec-1",
            "title": "Run command",
            "kind": "execute",
            "status": "pending",
            "rawInput": {
                "command": "/opt/norma/bin/norma brain report --text 'Alan 的登录页已完成，12 个测试全部通过。还需要你确认一下：旧登录接口要保留还是移除？'",
                "cwd": "/work/proj"
            }
        },
        "options": []
    });
    let request = PermissionRequest::from_params(&params);
    assert_eq!(gate().judge(&request), Verdict::Allow, "{request:?}");
}

/// 引号包住整条命令表示一个带空格的可执行文件名，不能再拆成受信任的 norma argv。
///
/// 数据构造：包了一层双引号的 norma brain report；包了一层的 `norma brain agents; rm -rf ~`。
/// 预期结果：
/// 执行过程：判断合法命令、单层与双层引号命令。
///   - 正断言：未包裹的 norma 命令放行
///   - 负断言：所有整条包裹的命令均拒绝
#[test]
fn quoted_executable_names_are_never_reinterpreted_as_commands() {
    let g = gate();
    assert_eq!(
        g.judge(&bash("/opt/norma/bin/norma brain agents")),
        Verdict::Allow,
        "正常命令应继续放行"
    );
    assert!(
        matches!(
            g.judge(&bash(r#""/opt/norma/bin/norma brain agents""#)),
            Verdict::Reject(_)
        ),
        "不能把带空格的可执行文件名拆成 norma 命令"
    );
    assert!(
        matches!(
            g.judge(&bash(r#""/opt/norma/bin/norma brain agents; rm -rf ~""#)),
            Verdict::Reject(_)
        ),
        "引号内的组合文本不能重新解释成命令"
    );
    assert!(
        matches!(
            g.judge(&bash(r#"'"/opt/norma/bin/norma brain agents"'"#)),
            Verdict::Reject(_)
        ),
        "多层引号不能触发递归拆包"
    );
}

/// ACP argv 数组的参数边界必须保留，不能把一个参数重新拆成子命令。
/// 数据构造：相同 norma 程序的正常 argv 与把 brain/agents 合成一项的 argv。
/// 执行过程：经 ACP 解析后交给 Gate。
/// 预期结果：正常 argv 放行，合并参数和非字符串参数拒绝。
#[test]
fn argv_boundaries_survive_permission_decoding() {
    let check = |argv: Value| {
        gate().judge(&PermissionRequest::from_params(&json!({
            "toolCall": {"kind": "execute", "rawInput": {"command": argv}}
        })))
    };
    assert_eq!(
        check(json!(["/opt/norma/bin/norma", "brain", "agents"])),
        Verdict::Allow,
        "正常 argv 应放行"
    );
    for argv in [
        json!(["/opt/norma/bin/norma", "brain agents"]),
        json!(["/opt/norma/bin/norma", null, "brain", "agents"]),
    ] {
        assert!(
            matches!(check(argv), Verdict::Reject(_)),
            "不能重新解释或丢弃 argv 元素"
        );
    }
}

/// 单次审批不能升级为整会话允许。
/// 数据构造：allow_always 排在 allow_once 前，以及仅包含 allow_always 的选项。
/// 执行过程：为 Allow 判定选择返回选项。
/// 预期结果：只选 allow_once；没有单次允许时取消。
#[test]
fn permission_grants_never_outlive_the_current_call() {
    let options = json!({"options": [
        {"kind": "allow_always", "optionId": "session"},
        {"kind": "allow_once", "optionId": "once"}
    ]});
    assert_eq!(
        pick_option(&options, &Verdict::Allow).as_deref(),
        Some("once"),
        "每条命令都必须重新经过闸门"
    );
    assert_eq!(
        pick_option(
            &json!({"options": [{"kind":"allow_always","optionId":"session"}]}),
            &Verdict::Allow
        ),
        None,
        "不得降级到整会话授权"
    );
}
