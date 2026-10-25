use super::{Gate, PermissionRequest, Verdict};
use serde_json::json;

/// 场景：真实 Codex unifiedExecStartup 权限请求携带 exec_command 工具名。
/// 数据构造：同一受控 shell envelope，仅替换工具名及 kind。
/// 执行过程：经过公开 ACP 解码与 Gate；预期仅已知 execute 别名放行。
/// 负断言：write_stdin、任意工具以及不匹配的 kind 仍拒绝。
#[test]
fn codex_execute_alias_requires_the_explicit_envelope() {
    let gate = Gate {
        norma_bin: "/opt/norma/bin/norma".into(),
        project_root: "/work/proj".into(),
    };
    for (name, kind, allow) in [
        ("exec_command", "execute", true),
        ("write_stdin", "execute", false),
        ("other", "execute", false),
        ("exec_command", "edit", false),
    ] {
        let request = PermissionRequest::from_params(&json!({"toolCall": {
            "name": name, "kind": kind,
            "rawInput": {"command": {"normaShellCommand": "/bin/zsh -lc '/opt/norma/bin/norma brain agents'"}}
        }}));
        assert_eq!(
            matches!(gate.judge(&request), Verdict::Allow),
            allow,
            "工具名 {name}，kind {kind}"
        );
    }
}

/// 场景：汇报正文引用是数据，不能因解包丢掉引号、反斜杠或字面 shell 符号。
/// 数据构造：单/双引号正文与字面 $()、分号；分别用合法两种外层包装。
/// 执行过程：ACP 解码后比较完整脚本，再解析最终 argv。
/// 预期结果：正文逐字一致；对象混入其它命令字段则拒绝。
#[test]
fn report_text_is_lossless_and_envelope_is_unambiguous() {
    for (script, body) in [
        (
            r#"/opt/norma/bin/norma brain report --text '他说："完成"'"#,
            "他说：\"完成\"",
        ),
        (
            r#"/opt/norma/bin/norma brain report --text "it's ready\\path""#,
            "it's ready\\path",
        ),
        (
            r#"/opt/norma/bin/norma brain report --text 'literal $(id); | & > \path'"#,
            r"literal $(id); | & > \path",
        ),
    ] {
        let single = format!("/bin/zsh -lc '{}'", script.replace('\'', "'\\''"));
        let double = format!(
            "/bin/bash -c \"{}\"",
            script
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
        );
        for command in [single, double] {
            let request = PermissionRequest::from_params(&json!({"toolCall": {
                "kind": "execute", "rawInput": {"command": {"normaShellCommand": command}}
            }}));
            assert_eq!(
                request.command.as_deref(),
                Some(script),
                "完整脚本不能被改写"
            );
            let argv = crate::risk::single_command(request.command.as_deref().unwrap()).unwrap();
            assert_eq!(argv[4], body, "正文必须逐字保真");
        }
    }
    let ambiguous = PermissionRequest::from_params(&json!({"toolCall": {
        "kind": "execute", "rawInput": {"command": {
            "normaShellCommand": "/bin/zsh -lc '/opt/norma/bin/norma brain agents'", "command": "echo other"
        }}
    }}));
    assert!(
        ambiguous.command.is_none(),
        "多字段包装必须拒绝，不能猜哪条才是实际命令"
    );
}

/// 场景：Codex 通过 shell 执行合法汇报，权限适配必须保留 shell 包装和正文引号的边界。
/// 数据构造：原始 shell 命令分别使用单引号、双引号包装，正文含中文及两种引用。
/// 执行过程：经 ACP 权限请求解码和真实 Gate 判断，与普通裸命令对照。
/// 预期结果：合法单条 norma brain 汇报放行；不会把外层包装当成可执行文件名。
#[test]
fn explicit_shell_envelopes_preserve_quoted_report_arguments() {
    let gate = Gate {
        norma_bin: "/opt/norma/bin/norma".into(),
        project_root: "/work/proj".into(),
    };
    for command in [
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents'",
        r#"/bin/zsh -lc "/opt/norma/bin/norma brain report --text 'Alan 还在处理中'""#,
        r#"/bin/bash -c '/opt/norma/bin/norma brain report --text "他说：可以继续"'"#,
        r#"/bin/zsh -lc "/opt/norma/bin/norma brain report --text '他说：\"可以\"，it'\\''s ready'""#,
    ] {
        let request = PermissionRequest::from_params(&json!({
            "toolCall": {"kind": "execute", "rawInput": {
                "command": {"normaShellCommand": command}
            }}
        }));
        assert_eq!(
            gate.judge(&request),
            Verdict::Allow,
            "应保留原始 shell 边界：{command}"
        );
    }
}

/// 场景：外层包装不是授权，嵌套 shell、其他程序和组合命令均不能借包装绕过闸门。
/// 数据构造：受信任 shell 的危险正文，以及同名伪 shell、额外参数、动态展开与控制字符。
/// 执行过程：全部经过和合法 report 相同的 ACP 解码及 Gate。
/// 预期结果：每个反例都拒绝，裸字符串的整条引号也保持拒绝。
#[test]
fn shell_envelopes_never_expand_the_command_allowlist() {
    let gate = Gate {
        norma_bin: "/opt/norma/bin/norma".into(),
        project_root: "/work/proj".into(),
    };
    for command in [
        "/bin/zsh -lc 'echo hello'",
        "/bin/zsh -lc '/tmp/norma brain agents'",
        "/bin/zsh -lc '/opt/norma/bin/norma card send 1 hello'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents; echo no'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents | cat'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents > /tmp/out'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents && echo no'",
        "/bin/zsh -lc 'sh -c /opt/norma/bin/norma'",
        "/tmp/zsh -lc '/opt/norma/bin/norma brain agents'",
        "zsh -lc '/opt/norma/bin/norma brain agents'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain agents' extra",
        "/bin/zsh -ic '/opt/norma/bin/norma brain agents'",
        r#"/bin/zsh -lc "/opt/norma/bin/norma brain report --text $SECRET""#,
        "/bin/zsh -lc '/opt/norma/bin/norma brain report --text $(id)'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain report --text `id`'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain report --text a\0b'",
        "/bin/zsh -lc '/opt/norma/bin/norma brain report --text a\rb'",
        r#"/bin/zsh -lc '"/opt/norma/bin/norma brain agents"'"#,
        r#"/bin/zsh -lc '"/opt/norma/bin/\norma" brain agents'"#,
    ] {
        let request = PermissionRequest::from_params(&json!({"toolCall": {
            "kind": "execute", "rawInput": {"command": {"normaShellCommand": command}}
        }}));
        assert!(
            matches!(gate.judge(&request), Verdict::Reject(_)),
            "必须拒绝：{command:?}"
        );
    }
    let quoted = PermissionRequest::from_params(&json!({"toolCall": {
        "kind": "execute", "rawInput": {"command": "\"/opt/norma/bin/norma brain agents\""}
    }}));
    assert!(
        matches!(gate.judge(&quoted), Verdict::Reject(_)),
        "裸字符串不能猜测包装来源"
    );
}
