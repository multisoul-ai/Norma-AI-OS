use super::Adapter;
use std::io::Write;
use std::process::Command;

/// 场景：模块加载失败时默认 stderr 不可见，握手必须收到可诊断错误。
/// 数据构造：具有正确版本与权限语句，但初始化抛错的 ESM 适配器。
/// 执行过程：启动生产 loader，向 stdin 发送 initialize 并读取 JSON-RPC。
/// 预期结果：返回对应请求的明确错误，不是无声退出或超时。
#[test]
fn loader_failure_is_visible_on_the_acp_channel() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dist")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"@agentclientprotocol/codex-acp","version":"2.2.2","type":"module"}"#,
    )
    .unwrap();
    let entry = dir.path().join("dist/index.js");
    std::fs::write(
        &entry,
        "// command: stripShellPrefix(params.command)\n// method: \"thread/start\", params }\nconst _hold = `const response = await this.codexClient.threadStart({\n      config: sessionConfig.config,`;\nthrow new Error('probe-load-failure');",
    )
    .unwrap();
    let adapter = Adapter::prepare(&entry, &[]).unwrap();
    let mut child = Command::new(&adapter.program)
        .args(&adapter.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\"}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ACP 通道必须返回错误正文");
    assert_eq!(response["id"], 7, "错误必须关联握手请求");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("probe-load-failure"),
        "保留明确失败原因"
    );
    assert!(response.get("result").is_none(), "失败不得冒充握手成功");
}

/// 场景：加载兼容适配器时必须保留原始 shell 请求及原模块 URL，不修改安装文件。
/// 数据构造：最小 ESM 适配器，具有钉死包名、版本和真实权限转换语句。
/// 执行过程：通过加载器运行 Node，检查 stdout，再改变版本验证失败关闭。
/// 预期结果：原始命令不损失引号，模块定位不变，未知版本拒绝准备。
#[test]
fn loader_preserves_authoritative_command_and_module_location() {
    let dir = tempfile::tempdir().expect("创建适配器夹具");
    std::fs::create_dir(dir.path().join("dist")).expect("创建 dist");
    let manifest = dir.path().join("package.json");
    std::fs::write(
        &manifest,
        r#"{"name":"@agentclientprotocol/codex-acp","version":"2.2.2","type":"module"}"#,
    )
    .expect("写包身份");
    let entry = dir.path().join("dist/index.js");
    let source = "const params = {command: process.argv[2]};\nfunction stripShellPrefix() { return \"lossy\"; }\nfunction threadStart(params) { return { method: \"thread/start\", params }; }\nconst _shared = `const response = await this.codexClient.threadStart({\n      config: sessionConfig.config,`;\nconsole.log(JSON.stringify({command: stripShellPrefix(params.command), thread: threadStart({cwd: \"/p\"}), url: import.meta.url}));";
    std::fs::write(&entry, source).expect("写适配器");
    let command = r#"/bin/zsh -lc "/opt/norma/bin/norma brain report --text '完成'""#;
    let adapter = Adapter::prepare(&entry, &[command.into()]).expect("准备兼容加载器");
    let output = Command::new(&adapter.program)
        .args(&adapter.args)
        .output()
        .expect("运行 Node 22.15+");
    assert!(
        output.status.success(),
        "加载器应成功：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("读取输出");
    assert_eq!(
        result["command"]["normaShellCommand"], command,
        "原始权限命令须逐字保留"
    );
    assert_eq!(
        result["thread"]["params"],
        serde_json::json!({"cwd": "/p", "ephemeral": true}),
        "大脑线程不能落盘进用户的 Codex 会话列表"
    );
    assert!(
        result["url"].as_str().unwrap().ends_with("/dist/index.js"),
        "相对依赖必须仍以原模块为基准"
    );
    assert_eq!(
        std::fs::read_to_string(&entry).unwrap(),
        source,
        "不能修改用户安装包"
    );
    std::fs::write(
        &manifest,
        r#"{"name":"@agentclientprotocol/codex-acp","version":"9.0.0"}"#,
    )
    .unwrap();
    assert!(Adapter::prepare(&entry, &[]).is_err(), "未知版本应失败关闭");
}

/// 场景：幕僚应使用兼容的较新本机 Codex，以读取随包版本不认识的模型元数据。
/// 数据构造：钉死的 ACP 壳，以及 PATH 中返回新版本或旧版本的 Codex 夹具。
/// 执行过程：通过生产 loader 启动 ACP 壳，读取适配器实际消费的 CODEX_PATH。
/// 预期结果：只自动选较新版本，旧版本保留随包默认，显式路径优先。
#[cfg(unix)]
#[test]
fn loader_prefers_newer_codex_and_respects_explicit_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let dist = dir.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"@agentclientprotocol/codex-acp","version":"2.2.2","type":"module"}"#,
    )
    .unwrap();
    let entry = dist.join("index.js");
    std::fs::write(
        &entry,
        "// command: stripShellPrefix(params.command)\n// method: \"thread/start\", params }\nconst _shared = `const response = await this.codexClient.threadStart({\n      config: sessionConfig.config,`;\nconsole.log(JSON.stringify(process.env.CODEX_PATH ?? null));",
    )
    .unwrap();

    let codex = dir.path().join("codex");
    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&inherited_path)),
    )
    .unwrap();
    let adapter = Adapter::prepare(&entry, &[]).unwrap();
    for (version, explicit, expected) in [
        ("0.171.0", None, Some("codex")),
        ("0.160.1", None, None),
        ("0.159.9", None, None),
        ("0.171.0", Some("/explicit/codex"), Some("/explicit/codex")),
    ] {
        std::fs::write(
            &codex,
            format!("#!/bin/sh\nprintf '%s\\n' 'codex-cli {version}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = Command::new(&adapter.program);
        command
            .args(&adapter.args)
            .env("PATH", &path)
            .env_remove("CODEX_PATH");
        if let Some(explicit) = explicit {
            command.env("CODEX_PATH", explicit);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "版本 {version} 的适配器启动必须成功"
        );
        let chosen: Option<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            chosen.as_deref(),
            expected,
            "版本 {version}、显式路径 {explicit:?} 应使用预期的 Codex"
        );
    }
}

/// 场景：真实钉死 bundle 的 reporter→renderer→loader→Gate 不应损失命令或工具名。
/// 数据构造：从 NORMA_CODEX_ACP_BUNDLE 读取安装包，仅将启动模型替换为确定性权限回放。
/// 执行过程：使用真实 CommandReporter 与 AcpToolCallRenderer，覆盖启动前后两种工具名。
/// 预期结果：原始命令逐字保留并放行；生产转换语句只由生产 loader 修改。
#[test]
#[ignore = "需要本地 codex-acp 2.2.2 安装包；通过 NORMA_CODEX_ACP_BUNDLE 指定"]
fn pinned_bundle_permission_round_trip() {
    let original =
        std::path::PathBuf::from(std::env::var_os("NORMA_CODEX_ACP_BUNDLE").expect("指定安装包"));
    let source = std::fs::read_to_string(&original).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dist")).unwrap();
    std::fs::copy(
        original
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("package.json"),
        dir.path().join("package.json"),
    )
    .unwrap();
    let probe = r#"for (const started of [false, true]) {
        const facts = CommandReporter.permission({itemId: 'replay', command: process.argv[2]}, started, started ? 'exec_command' : undefined);
        console.log(JSON.stringify(new AcpToolCallRenderer({airClient: false, air: {}}).renderPermissionToolCall(facts)));
    }"#;
    assert_eq!(
        source.matches("  startAcpServer();").count(),
        1,
        "真实入口必须唯一"
    );
    let entry = dir.path().join("dist/index.js");
    std::fs::write(&entry, source.replace("  startAcpServer();", probe)).unwrap();
    let command = r#"/bin/zsh -lc "/opt/norma/bin/norma brain report --text '他说：\"完成\"'""#;
    let adapter = Adapter::prepare(&entry, &[command.into()]).unwrap();
    let output = Command::new(&adapter.program)
        .args(&adapter.args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "真实包回放失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<_> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 2, "启动前后的两类请求都要覆盖");
    for line in lines {
        let call: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            call["rawInput"]["command"]["normaShellCommand"], command,
            "包转换不能丢引号"
        );
    }
}
