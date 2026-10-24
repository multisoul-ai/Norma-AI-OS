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

/// 读取只限工作空间内，且不能读密钥。
///
/// 数据构造：项目根 /work/proj。
/// 预期结果：
///   - 正断言：/work/proj/docs/a.md 和相对路径 src/a.rs 放行
///   - 负断言：/etc/hosts、`../x`、/work/proj/.env、缺路径都被拒绝
#[test]
fn reads_are_limited_to_the_project_and_never_secrets() {
    let g = gate();
    assert_eq!(g.judge(&read("/work/proj/docs/a.md")), Verdict::Allow);
    assert_eq!(g.judge(&read("src/a.rs")), Verdict::Allow);
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
        ("Bash", Some("bash -lc norma brain report"))
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

/// Codex 把整条命令再用双引号包一层发来；拆开一层后照常检查，组合命令仍被拒绝。
///
/// 数据构造：包了一层双引号的 norma brain report；包了一层的 `norma brain agents; rm -rf ~`。
/// 预期结果：
///   - 正断言：前者放行
///   - 负断言：后者被拒绝；包了两层的不再拆
#[test]
fn codex_quoted_command_is_unwrapped_once() {
    let g = gate();
    assert_eq!(
        g.judge(&bash(
            r#""/opt/norma/bin/norma brain report --text '做完了，需要你确认。'""#
        )),
        Verdict::Allow
    );
    assert!(matches!(
        g.judge(&bash(r#""/opt/norma/bin/norma brain agents; rm -rf ~""#)),
        Verdict::Reject(_)
    ));
    assert!(matches!(
        g.judge(&bash(r#"'"/opt/norma/bin/norma brain agents"'"#)),
        Verdict::Reject(_)
    ));
}
